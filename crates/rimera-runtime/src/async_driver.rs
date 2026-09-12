use std::ffi::c_void;
use std::fmt;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::ptr::NonNull;
use std::task::{Context, Poll};

use rimera_abi::{
    RAsyncCancelState, RAsyncCancelTransition, RAsyncCancelTransitionStatus, RAsyncCompletion,
    RAsyncCompletionKind, RAsyncDriverError, RAsyncOpaqueValue, RAsyncPollContext, RAsyncPollState,
    RAsyncTaskId, RAsyncWakeStatus, RGeneratorOperation, RGeneratorOutcome, RValue,
};
use rimera_async_runtime::{LocalBackend, LocalExecutorFacade, RootTask, TaskError, TaskStats};

use crate::RimeraContext;
use crate::heap::HeapObject;
use crate::object::SuspendedKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeTaskError {
    NotCoroutine,
    Executor(TaskError),
}

impl fmt::Display for RuntimeTaskError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotCoroutine => formatter.write_str("async root must be a native coroutine"),
            Self::Executor(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RuntimeTaskError {}

impl From<TaskError> for RuntimeTaskError {
    fn from(error: TaskError) -> Self {
        Self::Executor(error)
    }
}

/// Runtime-owned completion guard. Managed return values/exceptions remain
/// context roots until this guard is dropped; the executor facade transports
/// only their opaque 16-byte ABI representation.
pub struct RuntimeAsyncCompletion<'context> {
    task: RAsyncTaskId,
    kind: RAsyncCompletionKind,
    value: Option<RValue>,
    root: Option<CompletionRoot<'context>>,
}

impl fmt::Debug for RuntimeAsyncCompletion<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeAsyncCompletion")
            .field("task", &self.task)
            .field("kind", &self.kind)
            .field("value", &self.value)
            .finish_non_exhaustive()
    }
}

impl RuntimeAsyncCompletion<'_> {
    #[must_use]
    pub const fn task(&self) -> RAsyncTaskId {
        self.task
    }

    #[must_use]
    pub const fn kind(&self) -> RAsyncCompletionKind {
        self.kind
    }

    #[must_use]
    pub const fn value(&self) -> Option<RValue> {
        self.value
    }

    #[must_use]
    pub fn keeps_managed_value_rooted(&self) -> bool {
        self.root.is_some()
    }
}

struct CompletionRoot<'context> {
    context: NonNull<RimeraContext>,
    value: RValue,
    _lifetime: PhantomData<&'context RimeraContext>,
}

impl Drop for CompletionRoot<'_> {
    fn drop(&mut self) {
        // SAFETY: the completion lifetime is tied to the context borrow that
        // created the driver. The local async facade cannot cross threads.
        let context = unsafe { self.context.as_mut() };
        let removed = context.remove_context_root(self.value);
        debug_assert!(removed, "async completion root must remain registered");
    }
}

struct RootRuntimeState {
    context: NonNull<RimeraContext>,
    coroutine: RValue,
    coroutine_rooted: bool,
    completion_root: Option<RValue>,
    /// Managed memoryviews that pin the exact exporters handed to the current
    /// async operation. Registration may allocate once; poll/resume never does
    /// adapter-side lease bookkeeping or exposes a raw exporter pointer.
    buffer_leases: Vec<RValue>,
    terminal: bool,
    cancellation_observed: bool,
    cancellation_acknowledged: bool,
    cleanup_called: bool,
}

impl RootRuntimeState {
    fn root_completion(&mut self, value: RValue) {
        if value.handle_parts().is_none() {
            return;
        }
        debug_assert!(self.completion_root.is_none());
        // SAFETY: the owning RuntimeRootTask keeps the context alive and the
        // executor is strictly local-thread in this slice.
        let context = unsafe { self.context.as_mut() };
        context.add_context_root(value);
        self.completion_root = Some(value);
    }

    fn lease_buffer(&mut self, exporter: RValue) -> Result<RValue, String> {
        // Reserve adapter bookkeeping before acquiring the managed export. That
        // way a fallible Vec growth can never strand a rooted memoryview or an
        // exporter pin after the buffer protocol has already succeeded.
        self.buffer_leases.try_reserve(1).map_err(|error| {
            format!("async buffer lease bookkeeping allocation failed: {error}")
        })?;
        // Reuse the existing PEP 688/native memoryview ownership model rather
        // than inventing an async-only buffer representation. A live memoryview
        // pins bytearray resizing and retains provider/exporter/lease children
        // through normal GC tracing. No native borrow escapes this guard.
        let context = unsafe { self.context.as_mut() };
        let view = crate::operations::memoryview(context, exporter)?;
        context.add_context_root(view);
        self.buffer_leases.push(view);
        Ok(view)
    }

    fn release_buffer_leases(&mut self) {
        if self.buffer_leases.is_empty() {
            return;
        }
        // Temporarily detach the vector so release callbacks can use the
        // context without aliasing this state, then retain its allocation for
        // the next operation. Repeated buffered operations should pay for
        // bookkeeping growth once rather than once per completion.
        let mut leases = std::mem::take(&mut self.buffer_leases);
        let context = unsafe { self.context.as_mut() };
        for view in leases.drain(..) {
            // PEP 688 release-callback failures are already handled as
            // unraisable by the memoryview layer. Once an operation completes
            // or cancellation is acknowledged the async adapter must never
            // retain a borrow into this view.
            let _ = crate::operations::memoryview_release(context, view);
            let removed = context.remove_context_root(view);
            debug_assert!(removed, "async buffer lease root must remain registered");
        }
        self.buffer_leases = leases;
    }
}

impl Drop for RootRuntimeState {
    fn drop(&mut self) {
        // SAFETY: RootRuntimeState is owned by RuntimeRootTask, whose lifetime
        // is tied to the context borrow used to construct AsyncRuntimeDriver.
        self.release_buffer_leases();
        let context = unsafe { self.context.as_mut() };
        if let Some(value) = self.completion_root.take() {
            let removed = context.remove_context_root(value);
            debug_assert!(removed, "async completion root must remain registered");
        }
        if self.coroutine_rooted {
            let removed = context.remove_context_root(self.coroutine);
            debug_assert!(removed, "async coroutine root must remain registered");
            self.coroutine_rooted = false;
        }
    }
}

/// Owns the Python-runtime side of the local root-task boundary while delegating
/// scheduling entirely to the selected backend. It contains no ready queue,
/// worker thread, channel, mutex, or cross-thread atomic.
pub struct AsyncRuntimeDriver<'context> {
    context: NonNull<RimeraContext>,
    facade: LocalExecutorFacade,
    _lifetime: PhantomData<&'context mut RimeraContext>,
}

impl fmt::Debug for AsyncRuntimeDriver<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AsyncRuntimeDriver")
            .field("facade", &self.facade)
            .finish_non_exhaustive()
    }
}

impl<'context> AsyncRuntimeDriver<'context> {
    #[must_use]
    pub fn new(context: &'context mut RimeraContext) -> Self {
        Self::with_capacity(context, 0)
    }

    #[must_use]
    pub fn with_capacity(context: &'context mut RimeraContext, capacity: usize) -> Self {
        Self {
            context: NonNull::from(context),
            facade: LocalExecutorFacade::with_capacity(capacity),
            _lifetime: PhantomData,
        }
    }

    pub fn submit_root(
        &self,
        coroutine: RValue,
    ) -> Result<RuntimeRootTask<'context>, RuntimeTaskError> {
        let mut context_pointer = self.context;
        // SAFETY: AsyncRuntimeDriver owns the unique context borrow for its
        // lifetime. Polling is local and backend-driven, never concurrent.
        let context = unsafe { context_pointer.as_mut() };
        let is_coroutine = matches!(
            context.heap.get(coroutine),
            Some(HeapObject::Generator(object))
                if object.kind == SuspendedKind::Coroutine
                    && !object.running
                    && !object.completed
                    && !object.closed
        );
        if !is_coroutine {
            return Err(RuntimeTaskError::NotCoroutine);
        }

        context.add_context_root(coroutine);
        let mut state = Box::new(RootRuntimeState {
            context: self.context,
            coroutine,
            coroutine_rooted: true,
            completion_root: None,
            buffer_leases: Vec::new(),
            terminal: false,
            cancellation_observed: false,
            cancellation_acknowledged: false,
            cleanup_called: false,
        });
        let runtime = std::ptr::from_mut(state.as_mut()).cast::<c_void>();
        let (task_id, task) =
            self.facade
                .submit_root(runtime, resume_runtime_root, drop_runtime_root)?;
        Ok(RuntimeRootTask {
            task,
            facade: self.facade.clone(),
            state,
            task_id,
            _lifetime: PhantomData,
        })
    }

    /// Submit and synchronously drive exactly one root on a caller-selected
    /// backend. The backend owns polling/scheduling; Rimera only owns the root
    /// task identity and Python runtime state. Concrete backend selection and
    /// CLI/static-link policy remain Gate 10 Slice 10 work.
    pub fn run_root<B: LocalBackend>(
        &self,
        backend: &B,
        coroutine: RValue,
    ) -> Result<RuntimeAsyncCompletion<'context>, RuntimeTaskError> {
        let task = self.submit_root(coroutine)?;
        Ok(backend.drive_root(task))
    }

    pub fn request_cancel(&self, task: RAsyncTaskId) -> Result<bool, RuntimeTaskError> {
        self.facade.request_cancel(task).map_err(Into::into)
    }

    #[must_use]
    pub fn wake(&self, task: RAsyncTaskId) -> RAsyncWakeStatus {
        self.facade.wake(task)
    }

    pub fn shutdown(&self) {
        self.facade.shutdown();
    }

    #[must_use]
    pub fn stats(&self) -> TaskStats {
        self.facade.stats()
    }

    #[must_use]
    pub fn active_task_count(&self) -> usize {
        self.facade.active_task_count()
    }
}

/// Future submitted to the eventual backend. Nested native coroutine awaits are
/// resolved by Rimera's runtime suspension machinery and do not create another
/// executor task.
pub struct RuntimeRootTask<'context> {
    // Field order is intentional: RootTask must run its drop callback while the
    // boxed runtime state is still alive.
    task: RootTask,
    facade: LocalExecutorFacade,
    state: Box<RootRuntimeState>,
    task_id: RAsyncTaskId,
    _lifetime: PhantomData<&'context RimeraContext>,
}

impl fmt::Debug for RuntimeRootTask<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeRootTask")
            .field("task_id", &self.task_id)
            .field("cancellation_observed", &self.state.cancellation_observed)
            .finish_non_exhaustive()
    }
}

impl RuntimeRootTask<'_> {
    #[must_use]
    pub const fn id(&self) -> RAsyncTaskId {
        self.task_id
    }

    #[must_use]
    pub fn cancellation_observed(&self) -> bool {
        self.state.cancellation_observed
    }

    #[must_use]
    pub fn cancellation_acknowledged(&self) -> bool {
        self.state.cancellation_acknowledged
    }

    /// Pins a managed/native buffer for the currently outstanding async
    /// operation. The returned value is a managed memoryview handle, not a raw
    /// pointer; backend callbacks must borrow through the runtime while the
    /// lease is active and must never retain a borrow after completion or
    /// cancellation acknowledgement.
    pub fn lease_buffer(&mut self, exporter: RValue) -> Result<RValue, String> {
        self.state.lease_buffer(exporter)
    }
}

impl<'context> Future for RuntimeRootTask<'context> {
    type Output = RuntimeAsyncCompletion<'context>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.as_mut().get_mut();
        let Poll::Ready(raw) = Pin::new(&mut this.task).poll(context) else {
            return Poll::Pending;
        };

        let stored = match this.facade.take_completion(this.task_id) {
            Ok(stored) => stored,
            Err(_) => RAsyncCompletion::driver_error(RAsyncDriverError::RuntimeContract),
        };
        debug_assert_eq!(raw.kind, stored.kind);
        let raw = stored;
        let value = matches!(
            raw.kind,
            RAsyncCompletionKind::Returned | RAsyncCompletionKind::Raised
        )
        .then(|| unpack_value(raw.payload));

        let root = value.and_then(|value| {
            (this.state.completion_root == Some(value)).then(|| {
                this.state.completion_root = None;
                CompletionRoot {
                    context: this.state.context,
                    value,
                    _lifetime: PhantomData,
                }
            })
        });

        Poll::Ready(RuntimeAsyncCompletion {
            task: this.task_id,
            kind: raw.kind,
            value,
            root,
        })
    }
}

fn pack_value(value: RValue) -> RAsyncOpaqueValue {
    RAsyncOpaqueValue {
        low: u64::from(value.tag) | (u64::from(value.flags) << 32),
        high: value.payload,
    }
}

fn unpack_value(value: RAsyncOpaqueValue) -> RValue {
    let low = value.low.to_le_bytes();
    RValue {
        tag: u32::from_le_bytes([low[0], low[1], low[2], low[3]]),
        flags: u32::from_le_bytes([low[4], low[5], low[6], low[7]]),
        payload: value.high,
    }
}

unsafe extern "C" fn resume_runtime_root(
    runtime: *mut c_void,
    poll: *const RAsyncPollContext,
    completion: *mut RAsyncCompletion,
) -> RAsyncPollState {
    if runtime.is_null() || poll.is_null() || completion.is_null() {
        if !completion.is_null() {
            // SAFETY: null was rejected for this branch's output write.
            unsafe {
                completion.write(RAsyncCompletion::driver_error(
                    RAsyncDriverError::RuntimeContract,
                ));
            }
        }
        return RAsyncPollState::Ready;
    }

    // SAFETY: submit_root passes a live boxed RootRuntimeState and RootTask
    // guarantees the callback is never invoked after state teardown.
    let state = unsafe { &mut *runtime.cast::<RootRuntimeState>() };
    // SAFETY: poll points to stack storage owned by RootTask::poll for this call.
    let poll = unsafe { *poll };
    let cancellation_request = if poll.cancellation.state == RAsyncCancelState::Requested {
        // Observation and acknowledgement are deliberately separate. Observe
        // reserves this request for the runtime; acknowledgement happens only
        // after the managed Throw boundary has returned and operation-owned
        // buffer leases can be released safely.
        let transition = unsafe {
            (poll.cancel_transition)(
                poll.control_data,
                poll.task,
                poll.cancellation.request_id,
                RAsyncCancelTransition::Observe,
            )
        };
        if transition != RAsyncCancelTransitionStatus::Applied {
            state.terminal = true;
            unsafe {
                completion.write(RAsyncCompletion::driver_error(
                    RAsyncDriverError::RuntimeContract,
                ));
            }
            return RAsyncPollState::Ready;
        }
        state.cancellation_observed = true;
        Some(poll.cancellation.request_id)
    } else {
        state.cancellation_observed |= matches!(
            poll.cancellation.state,
            RAsyncCancelState::Observed | RAsyncCancelState::Acknowledged
        );
        state.cancellation_acknowledged |=
            poll.cancellation.state == RAsyncCancelState::Acknowledged;
        None
    };
    if state.terminal {
        // SAFETY: completion is writable by callback contract.
        unsafe {
            completion.write(RAsyncCompletion::driver_error(
                RAsyncDriverError::RuntimeContract,
            ));
        }
        return RAsyncPollState::Ready;
    }

    let mut cancellation_exception = None;
    let (operation, input) = if let Some(request_id) = cancellation_request {
        // Cancellation is a managed language/runtime signal. We intentionally
        // do not expose or claim an asyncio.CancelledError API in Gate 10; an
        // internal BaseException instance is injected through the exact same
        // native Throw path used by coroutine.throw().
        let created = {
            // SAFETY: the driver lifetime keeps the context alive and the local
            // facade serializes poll entry on one thread.
            let context = unsafe { state.context.as_mut() };
            crate::operations::string(context, "Rimera async operation cancelled").and_then(
                |message| {
                    context.with_temporary_roots(&[message], |context| {
                        context.new_builtin_exception("BaseException", &[message])
                    })
                },
            )
        };
        let exception = match created {
            Ok(exception) => exception,
            Err(message) => {
                // Cancellation delivery itself may allocate the internal
                // managed exception. Under the managed heap limit, use the
                // kernel's preallocated MemoryError rather than converting an
                // ordinary Python allocation failure into a backend error.
                let raised = {
                    let context = unsafe { state.context.as_mut() };
                    if context.raised.is_none() && message == "managed heap limit exceeded" {
                        context.raise_emergency_memory_error();
                    }
                    let raised = context.raised;
                    if raised.is_some() {
                        // Completion transport owns the failure from here; do
                        // not leak active exception state into later runtime work.
                        context.raised = None;
                        context.exception = None;
                    }
                    raised
                };
                let transition = unsafe {
                    (poll.cancel_transition)(
                        poll.control_data,
                        poll.task,
                        request_id,
                        RAsyncCancelTransition::Acknowledge,
                    )
                };
                if transition != RAsyncCancelTransitionStatus::Applied {
                    state.release_buffer_leases();
                    state.terminal = true;
                    unsafe {
                        completion.write(RAsyncCompletion::driver_error(
                            RAsyncDriverError::RuntimeContract,
                        ));
                    }
                    return RAsyncPollState::Ready;
                }
                state.cancellation_acknowledged = true;
                state.release_buffer_leases();
                state.terminal = true;
                if let Some(raised) = raised {
                    state.root_completion(raised);
                    unsafe {
                        completion.write(RAsyncCompletion::new(
                            RAsyncCompletionKind::Raised,
                            pack_value(raised),
                        ));
                    }
                } else {
                    unsafe {
                        completion.write(RAsyncCompletion::driver_error(
                            RAsyncDriverError::RuntimeContract,
                        ));
                    }
                }
                return RAsyncPollState::Ready;
            }
        };
        cancellation_exception = Some(exception);
        (RGeneratorOperation::Throw, exception)
    } else {
        (RGeneratorOperation::Send, RValue::NONE)
    };

    let resume_result = {
        let context = unsafe { state.context.as_mut() };
        context.resume_generator(state.coroutine, operation, input)
    };

    if let Some(request_id) = cancellation_request {
        // The managed Throw boundary has returned, so the current operation no
        // longer owns any native/managed buffer borrow. Acknowledge transport
        // only now, then release the operation leases exactly once.
        let transition = unsafe {
            (poll.cancel_transition)(
                poll.control_data,
                poll.task,
                request_id,
                RAsyncCancelTransition::Acknowledge,
            )
        };
        if transition != RAsyncCancelTransitionStatus::Applied {
            state.release_buffer_leases();
            state.terminal = true;
            unsafe {
                completion.write(RAsyncCompletion::driver_error(
                    RAsyncDriverError::RuntimeContract,
                ));
            }
            return RAsyncPollState::Ready;
        }
        state.cancellation_acknowledged = true;
        state.release_buffer_leases();
    }

    match resume_result {
        Ok(result)
            if matches!(
                result.outcome,
                RGeneratorOutcome::Yielded | RGeneratorOutcome::Suspended
            ) =>
        {
            RAsyncPollState::Pending
        }
        Ok(result) => {
            state.release_buffer_leases();
            state.root_completion(result.value);
            state.terminal = true;
            unsafe {
                completion.write(RAsyncCompletion::new(
                    RAsyncCompletionKind::Returned,
                    pack_value(result.value),
                ));
            }
            RAsyncPollState::Ready
        }
        Err(_) => {
            let raised = unsafe { state.context.as_ref().raised };
            if raised.is_some() && raised == cancellation_exception {
                // An uncaught injected cancellation is a terminal task
                // cancellation, not a backend failure and not a generic Python
                // exception completion. Clear the driver-owned signal so it
                // cannot leak into unrelated later runtime work.
                let context = unsafe { state.context.as_mut() };
                context.raised = None;
                context.exception = None;
                state.release_buffer_leases();
                state.terminal = true;
                unsafe {
                    completion.write(RAsyncCompletion::new(
                        RAsyncCompletionKind::Cancelled,
                        RAsyncOpaqueValue::default(),
                    ));
                }
                return RAsyncPollState::Ready;
            }
            let Some(exception) = raised else {
                state.release_buffer_leases();
                state.terminal = true;
                unsafe {
                    completion.write(RAsyncCompletion::driver_error(
                        RAsyncDriverError::RuntimeContract,
                    ));
                }
                return RAsyncPollState::Ready;
            };
            // The exception is now represented by the rooted completion value;
            // active runtime exception state must not bleed into unrelated work.
            let context = unsafe { state.context.as_mut() };
            context.raised = None;
            context.exception = None;
            state.release_buffer_leases();
            state.root_completion(exception);
            state.terminal = true;
            unsafe {
                completion.write(RAsyncCompletion::new(
                    RAsyncCompletionKind::Raised,
                    pack_value(exception),
                ));
            }
            RAsyncPollState::Ready
        }
    }
}

unsafe extern "C" fn drop_runtime_root(runtime: *mut c_void, _task: RAsyncTaskId) {
    if runtime.is_null() {
        return;
    }
    // SAFETY: the pointer is the live boxed state paired with this callback at
    // submission and remains owned by RuntimeRootTask until RootTask drops.
    let state = unsafe { &mut *runtime.cast::<RootRuntimeState>() };
    if state.cleanup_called || state.terminal {
        return;
    }
    state.cleanup_called = true;
    // SAFETY: the driver lifetime keeps the context alive through task drop.
    {
        let context = unsafe { state.context.as_mut() };
        let previous_raised = context.raised.take();
        let previous_exception = context.exception.take();
        let _ = context.resume_generator(state.coroutine, RGeneratorOperation::Close, RValue::NONE);
        context.raised = previous_raised;
        context.exception = previous_exception;
    }
    // Keep operation-owned buffers valid through the managed Close boundary,
    // then release them exactly once before shutdown/drop returns.
    state.release_buffer_leases();
    state.terminal = true;
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;
    use std::task::Waker;
    use std::time::Instant;

    use rimera_abi::{RGeneratorOperation, RGeneratorOutcome, RStatus};

    use super::*;
    use crate::object::{CodeObject, FastCallMetadata, FunctionKind, FunctionObject};
    use crate::operations;

    unsafe extern "C" fn two_poll_coroutine(
        context: *mut c_void,
        generator: *const RValue,
        operation: RGeneratorOperation,
        _input: *const RValue,
        output: *mut RValue,
        outcome: *mut RGeneratorOutcome,
    ) -> RStatus {
        if context.is_null() || generator.is_null() || output.is_null() || outcome.is_null() {
            return RStatus::InvalidArgument;
        }
        // SAFETY: tests register this function as the native resume entry for a
        // live coroutine in this exact RimeraContext.
        let context = unsafe { &mut *context.cast::<RimeraContext>() };
        let generator = unsafe { *generator };
        if operation == RGeneratorOperation::Close {
            unsafe {
                output.write(RValue::NONE);
                outcome.write(RGeneratorOutcome::Returned);
            }
            return RStatus::Ok;
        }
        let Some(HeapObject::Generator(object)) = context.heap.get_mut(generator) else {
            return RStatus::InvalidArgument;
        };
        if object.state == 0 {
            object.state = 1;
            unsafe {
                output.write(RValue::small_int(7));
                outcome.write(RGeneratorOutcome::Yielded);
            }
            return RStatus::Ok;
        }
        let result = object
            .slots
            .first()
            .copied()
            .flatten()
            .unwrap_or(RValue::NONE);
        unsafe {
            output.write(result);
            outcome.write(RGeneratorOutcome::Returned);
        }
        RStatus::Ok
    }

    unsafe extern "C" fn throw_aware_coroutine(
        context: *mut c_void,
        generator: *const RValue,
        operation: RGeneratorOperation,
        input: *const RValue,
        output: *mut RValue,
        outcome: *mut RGeneratorOutcome,
    ) -> RStatus {
        if context.is_null()
            || generator.is_null()
            || input.is_null()
            || output.is_null()
            || outcome.is_null()
        {
            return RStatus::InvalidArgument;
        }
        let context = unsafe { &mut *context.cast::<RimeraContext>() };
        let generator = unsafe { *generator };
        if operation == RGeneratorOperation::Close {
            unsafe {
                output.write(RValue::NONE);
                outcome.write(RGeneratorOutcome::Returned);
            }
            return RStatus::Ok;
        }
        if operation == RGeneratorOperation::Throw {
            let exception = unsafe { *input };
            context.raised = Some(exception);
            context.exception = Some("uncaught async cancellation".to_owned());
            return RStatus::Exception;
        }
        let Some(HeapObject::Generator(object)) = context.heap.get_mut(generator) else {
            return RStatus::InvalidArgument;
        };
        if object.state == 0 {
            object.state = 1;
            unsafe {
                output.write(RValue::small_int(7));
                outcome.write(RGeneratorOutcome::Yielded);
            }
        } else {
            unsafe {
                output.write(RValue::small_int(41));
                outcome.write(RGeneratorOutcome::Returned);
            }
        }
        RStatus::Ok
    }

    unsafe extern "C" fn caught_throw_coroutine(
        context: *mut c_void,
        generator: *const RValue,
        operation: RGeneratorOperation,
        _input: *const RValue,
        output: *mut RValue,
        outcome: *mut RGeneratorOutcome,
    ) -> RStatus {
        if context.is_null() || generator.is_null() || output.is_null() || outcome.is_null() {
            return RStatus::InvalidArgument;
        }
        let context = unsafe { &mut *context.cast::<RimeraContext>() };
        let generator = unsafe { *generator };
        if operation == RGeneratorOperation::Close {
            unsafe {
                output.write(RValue::NONE);
                outcome.write(RGeneratorOutcome::Returned);
            }
            return RStatus::Ok;
        }
        let Some(HeapObject::Generator(object)) = context.heap.get_mut(generator) else {
            return RStatus::InvalidArgument;
        };
        if object.state == 0 {
            object.state = 1;
            unsafe {
                output.write(RValue::small_int(7));
                outcome.write(RGeneratorOutcome::Yielded);
            }
            return RStatus::Ok;
        }
        unsafe {
            output.write(if operation == RGeneratorOperation::Throw {
                RValue::small_int(99)
            } else {
                RValue::small_int(41)
            });
            outcome.write(RGeneratorOutcome::Returned);
        }
        RStatus::Ok
    }

    unsafe extern "C" fn immediate_coroutine(
        _context: *mut c_void,
        _generator: *const RValue,
        operation: RGeneratorOperation,
        _input: *const RValue,
        output: *mut RValue,
        outcome: *mut RGeneratorOutcome,
    ) -> RStatus {
        if output.is_null() || outcome.is_null() {
            return RStatus::InvalidArgument;
        }
        unsafe {
            output.write(if operation == RGeneratorOperation::Close {
                RValue::NONE
            } else {
                RValue::small_int(42)
            });
            outcome.write(RGeneratorOutcome::Returned);
        }
        RStatus::Ok
    }

    fn make_coroutine_with_code(
        context: &mut RimeraContext,
        result: RValue,
        code_address: usize,
    ) -> RValue {
        let globals = context.globals().expect("initialized runtime globals");
        let code = context
            .allocate(HeapObject::Code(CodeObject {
                dynamic_mode: None,
                flags_override: None,
                code_address,
                kind: FunctionKind::Coroutine {
                    persistent_slot_count: 1,
                },
                name: "root".to_owned(),
                qualified_name: "root".to_owned(),
                parameters: Box::new([]),
                filename: "<gate10-slice4>".to_owned(),
                first_line: 1,
                local_names: Box::new([]),
                cell_names: Box::new([]),
                free_names: Box::new([]),
            }))
            .unwrap();
        context.add_context_root(code);
        let function = context
            .allocate(HeapObject::Function(FunctionObject {
                code,
                fast_call: FastCallMetadata {
                    positional_arity: Some(0),
                    kind: FunctionKind::Coroutine {
                        persistent_slot_count: 1,
                    },
                    code_address,
                    first_line: 1,
                    ready_coroutine_code_address: None,
                    ready_coroutine_repeat_pure: false,
                },
                globals,
                name: "root".to_owned(),
                qualified_name: "root".to_owned(),
                closure: None,
                defaults: None,
                keyword_defaults: None,
                annotations: None,
                type_params: None,
            }))
            .unwrap();
        context.add_context_root(function);
        context.remove_context_root(code);
        let coroutine = context.new_coroutine(function, &[result]).unwrap();
        context.remove_context_root(function);
        coroutine
    }

    fn make_coroutine(context: &mut RimeraContext, result: RValue) -> RValue {
        make_coroutine_with_code(context, result, two_poll_coroutine as usize)
    }

    #[derive(Debug, Default)]
    struct ImmediateBackend;

    impl LocalBackend for ImmediateBackend {
        type Error = Infallible;
        type TimerHandle = ();

        const NAME: &'static str = "immediate-test";

        fn new() -> Result<Self, Self::Error> {
            Ok(Self)
        }

        fn drive_root<F: Future>(&self, future: F) -> F::Output {
            let mut future = Box::pin(future);
            let waker = Waker::noop();
            let mut context = Context::from_waker(waker);
            match future.as_mut().poll(&mut context) {
                Poll::Ready(value) => value,
                Poll::Pending => panic!("immediate backend received a pending root"),
            }
        }

        fn register_timer<F>(&self, _deadline: Instant, _on_fire: F) -> Self::TimerHandle
        where
            F: FnOnce() + 'static,
        {
            panic!("immediate backend does not provide timers")
        }
    }

    fn poll_once<'context>(
        task: &mut RuntimeRootTask<'context>,
    ) -> Poll<RuntimeAsyncCompletion<'context>> {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        Pin::new(task).poll(&mut context)
    }

    #[test]
    fn root_task_keeps_coroutine_and_completion_gc_safe_across_backend_handoff() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let result = operations::string(&mut context, "kept-through-completion").unwrap();
        let coroutine = context
            .with_temporary_roots(&[result], |context| {
                Ok::<_, String>(make_coroutine(context, result))
            })
            .unwrap();
        let live_before_driver = context.heap.live_bytes();
        context.set_heap_limit(Some(live_before_driver)).unwrap();

        let mut driver = AsyncRuntimeDriver::with_capacity(&mut context, 1);
        let mut task = driver.submit_root(coroutine).unwrap();
        assert!(matches!(poll_once(&mut task), Poll::Pending));
        assert_eq!(driver.active_task_count(), 1);

        // SAFETY: the driver owns the unique context borrow; this test invokes
        // collection between backend polls on that same local thread.
        unsafe { driver.context.as_mut().collect() };
        assert!(unsafe { driver.context.as_ref().heap.get(coroutine).is_some() });
        assert!(unsafe { driver.context.as_ref().heap.get(result).is_some() });

        assert_eq!(driver.wake(task.id()), RAsyncWakeStatus::Woken);
        let Poll::Ready(completion) = poll_once(&mut task) else {
            panic!("second resume must complete the test coroutine");
        };
        assert_eq!(completion.kind(), RAsyncCompletionKind::Returned);
        assert_eq!(completion.value(), Some(result));
        assert!(completion.keeps_managed_value_rooted());
        assert_eq!(driver.active_task_count(), 0);
        assert_eq!(driver.stats().submitted, 1);
        assert_eq!(driver.stats().completions, 1);
        assert_eq!(driver.stats().rimera_wrapper_allocations, 0);

        unsafe { driver.context.as_mut().collect() };
        assert!(unsafe { driver.context.as_ref().heap.get(result).is_some() });
        drop(completion);
        drop(task);
        unsafe { driver.context.as_mut().collect() };
        assert!(unsafe { driver.context.as_ref().heap.get(result).is_none() });
        assert!(unsafe { driver.context.as_ref().heap.get(coroutine).is_none() });
    }

    #[test]
    fn cancellation_is_thrown_acknowledged_once_and_clears_runtime_exception_state() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let data = operations::bytearray(&mut context, b"cancelled-buffer").unwrap();
        let coroutine = context
            .with_temporary_roots(&[data], |context| {
                Ok::<_, String>(make_coroutine_with_code(
                    context,
                    RValue::NONE,
                    throw_aware_coroutine as usize,
                ))
            })
            .unwrap();
        let driver = AsyncRuntimeDriver::with_capacity(&mut context, 1);
        let mut task = driver.submit_root(coroutine).unwrap();
        assert!(matches!(poll_once(&mut task), Poll::Pending));
        task.lease_buffer(data).unwrap();
        assert_eq!(
            match unsafe { driver.context.as_ref().heap.get(data) } {
                Some(HeapObject::ByteArray(bytes)) => bytes.exports,
                _ => panic!("cancelled buffer disappeared"),
            },
            1
        );
        assert!(driver.request_cancel(task.id()).unwrap());
        assert!(!driver.request_cancel(task.id()).unwrap());
        let Poll::Ready(completion) = poll_once(&mut task) else {
            panic!("uncaught cancellation must terminate the root");
        };
        assert_eq!(completion.kind(), RAsyncCompletionKind::Cancelled);
        assert!(task.cancellation_observed());
        assert!(task.cancellation_acknowledged());
        let stats = driver.stats();
        assert_eq!(stats.cancellation_requests, 1);
        assert_eq!(stats.cancellation_observations, 1);
        assert_eq!(stats.cancellation_acknowledgements, 1);
        assert_eq!(
            match unsafe { driver.context.as_ref().heap.get(data) } {
                Some(HeapObject::ByteArray(bytes)) => bytes.exports,
                _ => panic!("cancelled buffer disappeared before release proof"),
            },
            0
        );
        assert!(unsafe { driver.context.as_ref().raised.is_none() });
        assert!(unsafe { driver.context.as_ref().exception.is_none() });
    }

    #[test]
    fn caught_cancellation_follows_normal_coroutine_return_semantics() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let coroutine =
            make_coroutine_with_code(&mut context, RValue::NONE, caught_throw_coroutine as usize);
        let driver = AsyncRuntimeDriver::with_capacity(&mut context, 1);
        let mut task = driver.submit_root(coroutine).unwrap();
        assert!(matches!(poll_once(&mut task), Poll::Pending));
        assert!(driver.request_cancel(task.id()).unwrap());
        let Poll::Ready(completion) = poll_once(&mut task) else {
            panic!("caught cancellation must return normally");
        };
        assert_eq!(completion.kind(), RAsyncCompletionKind::Returned);
        assert_eq!(completion.value(), Some(RValue::small_int(99)));
        assert!(task.cancellation_observed());
        assert!(task.cancellation_acknowledged());
        assert_eq!(driver.stats().cancellation_observations, 1);
        assert_eq!(driver.stats().cancellation_acknowledgements, 1);
        assert!(unsafe { driver.context.as_ref().raised.is_none() });
    }

    #[test]
    fn cancellation_exception_allocation_failure_is_a_managed_memory_error() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let coroutine =
            make_coroutine_with_code(&mut context, RValue::NONE, throw_aware_coroutine as usize);
        let mut driver = AsyncRuntimeDriver::with_capacity(&mut context, 1);
        let mut task = driver.submit_root(coroutine).unwrap();
        assert!(matches!(poll_once(&mut task), Poll::Pending));
        let live = unsafe { driver.context.as_ref().heap.live_bytes() };
        unsafe { driver.context.as_mut().set_heap_limit(Some(live)).unwrap() };
        assert!(driver.request_cancel(task.id()).unwrap());
        let Poll::Ready(completion) = poll_once(&mut task) else {
            panic!("low-heap cancellation delivery must terminate");
        };
        assert_eq!(completion.kind(), RAsyncCompletionKind::Raised);
        let exception = completion.value().expect("managed MemoryError completion");
        assert_eq!(
            unsafe { driver.context.as_ref().exception_type_name(exception) },
            Some("MemoryError")
        );
        assert!(task.cancellation_observed());
        assert!(task.cancellation_acknowledged());
        assert_eq!(driver.stats().cancellation_observations, 1);
        assert_eq!(driver.stats().cancellation_acknowledgements, 1);
        assert!(unsafe { driver.context.as_ref().raised.is_none() });
        assert!(unsafe { driver.context.as_ref().exception.is_none() });
        unsafe { driver.context.as_mut().set_heap_limit(None).unwrap() };
    }

    #[test]
    fn caller_selected_backend_drives_exactly_one_root_without_backend_selection_in_runtime() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let coroutine =
            make_coroutine_with_code(&mut context, RValue::NONE, immediate_coroutine as usize);
        let driver = AsyncRuntimeDriver::with_capacity(&mut context, 1);
        let completion = driver.run_root(&ImmediateBackend, coroutine).unwrap();
        assert_eq!(completion.kind(), RAsyncCompletionKind::Returned);
        assert_eq!(completion.value(), Some(RValue::small_int(42)));
        assert_eq!(driver.stats().submitted, 1);
        assert_eq!(driver.stats().completions, 1);
        assert_eq!(driver.active_task_count(), 0);
    }

    #[test]
    fn async_bytearray_lease_pins_resize_and_releases_on_completion() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let data = operations::bytearray(&mut context, b"abcd").unwrap();
        let coroutine = context
            .with_temporary_roots(&[data], |context| {
                Ok::<_, String>(make_coroutine(context, RValue::NONE))
            })
            .unwrap();
        let mut driver = AsyncRuntimeDriver::with_capacity(&mut context, 1);
        let mut task = driver.submit_root(coroutine).unwrap();
        assert!(matches!(poll_once(&mut task), Poll::Pending));
        let view = task.lease_buffer(data).unwrap();
        assert!(matches!(
            unsafe { driver.context.as_ref().heap.get(view) },
            Some(HeapObject::MemoryView(_))
        ));
        assert_eq!(
            match unsafe { driver.context.as_ref().heap.get(data) } {
                Some(HeapObject::ByteArray(bytes)) => bytes.exports,
                _ => panic!("leased bytearray disappeared"),
            },
            1
        );
        let slice = unsafe {
            operations::slice(
                driver.context.as_mut(),
                Some(RValue::small_int(0)),
                Some(RValue::small_int(1)),
                None,
            )
            .unwrap()
        };
        assert!(unsafe { operations::item_delete(driver.context.as_mut(), data, slice) }.is_err());
        assert!(unsafe {
            driver
                .context
                .as_mut()
                .consume_exception_type("BufferError")
        });

        assert_eq!(driver.wake(task.id()), RAsyncWakeStatus::Woken);
        assert!(matches!(poll_once(&mut task), Poll::Ready(_)));
        assert_eq!(
            match unsafe { driver.context.as_ref().heap.get(data) } {
                Some(HeapObject::ByteArray(bytes)) => bytes.exports,
                _ => panic!("bytearray disappeared before release assertion"),
            },
            0
        );
        assert!(unsafe { operations::item_delete(driver.context.as_mut(), data, slice) }.is_ok());
    }

    #[test]
    fn async_buffer_lease_failure_and_pending_drop_leave_no_export_behind() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let data = operations::bytearray(&mut context, b"abcd").unwrap();
        let coroutine = context
            .with_temporary_roots(&[data], |context| {
                Ok::<_, String>(make_coroutine(context, RValue::NONE))
            })
            .unwrap();
        let mut driver = AsyncRuntimeDriver::with_capacity(&mut context, 1);
        let mut task = driver.submit_root(coroutine).unwrap();
        assert!(matches!(poll_once(&mut task), Poll::Pending));
        let live = unsafe { driver.context.as_ref().heap.live_bytes() };
        unsafe { driver.context.as_mut().set_heap_limit(Some(live)).unwrap() };
        assert_eq!(
            task.lease_buffer(data).unwrap_err(),
            "managed heap limit exceeded"
        );
        assert_eq!(
            match unsafe { driver.context.as_ref().heap.get(data) } {
                Some(HeapObject::ByteArray(bytes)) => bytes.exports,
                _ => panic!("bytearray disappeared during failed lease"),
            },
            0
        );
        unsafe { driver.context.as_mut().set_heap_limit(None).unwrap() };
        task.lease_buffer(data).unwrap();
        assert_eq!(
            match unsafe { driver.context.as_ref().heap.get(data) } {
                Some(HeapObject::ByteArray(bytes)) => bytes.exports,
                _ => panic!("bytearray disappeared during live lease"),
            },
            1
        );
        drop(task);
        assert_eq!(
            match unsafe { driver.context.as_ref().heap.get(data) } {
                Some(HeapObject::ByteArray(bytes)) => bytes.exports,
                _ => panic!("bytearray disappeared before drop release assertion"),
            },
            0
        );
    }

    #[test]
    fn shutdown_releases_live_buffer_lease_before_task_drop() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let data = operations::bytearray(&mut context, b"shutdown-buffer").unwrap();
        let coroutine = context
            .with_temporary_roots(&[data], |context| {
                Ok::<_, String>(make_coroutine(context, RValue::NONE))
            })
            .unwrap();
        let driver = AsyncRuntimeDriver::with_capacity(&mut context, 1);
        let mut task = driver.submit_root(coroutine).unwrap();
        assert!(matches!(poll_once(&mut task), Poll::Pending));
        task.lease_buffer(data).unwrap();
        assert_eq!(
            match unsafe { driver.context.as_ref().heap.get(data) } {
                Some(HeapObject::ByteArray(bytes)) => bytes.exports,
                _ => panic!("shutdown buffer disappeared"),
            },
            1
        );

        driver.shutdown();
        assert_eq!(
            match unsafe { driver.context.as_ref().heap.get(data) } {
                Some(HeapObject::ByteArray(bytes)) => bytes.exports,
                _ => panic!("shutdown buffer disappeared before release"),
            },
            0
        );
        let Poll::Ready(completion) = poll_once(&mut task) else {
            panic!("shutdown must terminate the pending root");
        };
        assert_eq!(completion.kind(), RAsyncCompletionKind::DriverError);
        drop(completion);
        drop(task);
        assert_eq!(
            match unsafe { driver.context.as_ref().heap.get(data) } {
                Some(HeapObject::ByteArray(bytes)) => bytes.exports,
                _ => panic!("shutdown buffer disappeared after task drop"),
            },
            0
        );
    }

    #[test]
    fn dropping_pending_root_closes_once_and_reclaims_registered_roots() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let result = operations::string(&mut context, "pending-drop").unwrap();
        let coroutine = context
            .with_temporary_roots(&[result], |context| {
                Ok::<_, String>(make_coroutine(context, result))
            })
            .unwrap();
        let mut driver = AsyncRuntimeDriver::with_capacity(&mut context, 1);
        let mut task = driver.submit_root(coroutine).unwrap();
        assert!(matches!(poll_once(&mut task), Poll::Pending));
        drop(task);
        unsafe { driver.context.as_mut().collect() };
        assert!(unsafe { driver.context.as_ref().heap.get(coroutine).is_none() });
        assert!(unsafe { driver.context.as_ref().heap.get(result).is_none() });
    }
}
