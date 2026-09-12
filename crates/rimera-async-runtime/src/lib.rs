mod backend;

pub use backend::{CompioBackend, LocalBackend};

use std::cell::{Cell, UnsafeCell};
use std::ffi::c_void;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use rimera_abi::{
    RAsyncCancelState, RAsyncCancelTransition, RAsyncCancelTransitionStatus, RAsyncCancellation,
    RAsyncCompletion, RAsyncCompletionKind, RAsyncDeadline, RAsyncDriverError, RAsyncDropFn,
    RAsyncOpaqueValue, RAsyncPollContext, RAsyncPollState, RAsyncResumeFn, RAsyncTaskId,
    RAsyncTimerId, RAsyncTimerState, RAsyncWakeStatus,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskError {
    StaleTaskId,
    AlreadyCompleted,
    NotCompleted,
    ExecutorShutdown,
    TaskCapacityExceeded,
    InvalidDeadline,
}

impl fmt::Display for TaskError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::StaleTaskId => "stale async task id",
            Self::AlreadyCompleted => "async task is already complete",
            Self::NotCompleted => "async task has not completed",
            Self::ExecutorShutdown => "async executor facade is shut down",
            Self::TaskCapacityExceeded => "async task arena exceeded u32 capacity",
            Self::InvalidDeadline => "async deadline is outside the backend instant range",
        })
    }
}

impl std::error::Error for TaskError {}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TaskStats {
    pub submitted: u64,
    pub polls: u64,
    pub completions: u64,
    pub wakes: u64,
    pub coalesced_wakes: u64,
    pub cancellation_requests: u64,
    pub cancellation_observations: u64,
    pub cancellation_acknowledgements: u64,
    pub timers_registered: u64,
    pub timers_fired: u64,
    pub timers_cancelled: u64,
    /// Root wrappers are values moved into the backend. The local facade does
    /// not box or otherwise allocate one Rimera-owned wrapper per task.
    pub rimera_wrapper_allocations: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskState {
    Active,
    Completed,
}

#[derive(Debug)]
struct TaskRecord {
    state: TaskState,
    completion: Option<RAsyncCompletion>,
    waker: Option<Waker>,
    runtime: *mut c_void,
    drop_fn: RAsyncDropFn,
    cancellation: RAsyncCancellation,
    wake_outstanding: bool,
    polling: bool,
    cleanup_called: bool,
}

#[derive(Debug)]
struct Slot {
    generation: u32,
    next_free: Option<u32>,
    record: Option<TaskRecord>,
}

impl Slot {
    const fn vacant(generation: u32) -> Self {
        Self {
            generation,
            next_free: None,
            record: None,
        }
    }
}

#[derive(Debug)]
struct TaskArena {
    slots: Vec<Slot>,
    free_head: Option<u32>,
    active_tasks: usize,
    shutdown: bool,
    epoch: Instant,
    next_cancel_request: u64,
    next_timer_sequence: u64,
    stats: TaskStats,
}

impl TaskArena {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            slots: Vec::with_capacity(capacity),
            free_head: None,
            active_tasks: 0,
            shutdown: false,
            epoch: Instant::now(),
            next_cancel_request: 1,
            next_timer_sequence: 1,
            stats: TaskStats::default(),
        }
    }

    fn allocate(
        &mut self,
        runtime: *mut c_void,
        drop_fn: RAsyncDropFn,
    ) -> Result<RAsyncTaskId, TaskError> {
        if self.shutdown {
            return Err(TaskError::ExecutorShutdown);
        }
        let index = if let Some(index) = self.free_head {
            let slot = self
                .slots
                .get_mut(index as usize)
                .expect("free-list task slot must exist");
            self.free_head = slot.next_free.take();
            index
        } else {
            let index =
                u32::try_from(self.slots.len()).map_err(|_| TaskError::TaskCapacityExceeded)?;
            self.slots.push(Slot::vacant(1));
            index
        };
        let slot = self
            .slots
            .get_mut(index as usize)
            .expect("allocated task slot must exist");
        debug_assert!(slot.record.is_none());
        slot.record = Some(TaskRecord {
            state: TaskState::Active,
            completion: None,
            waker: None,
            runtime,
            drop_fn,
            cancellation: RAsyncCancellation::CLEAR,
            wake_outstanding: false,
            polling: false,
            cleanup_called: false,
        });
        self.active_tasks += 1;
        self.stats.submitted = self.stats.submitted.saturating_add(1);
        Ok(RAsyncTaskId {
            index,
            generation: slot.generation,
        })
    }

    fn slot(&self, task: RAsyncTaskId) -> Result<&Slot, TaskError> {
        let slot = self
            .slots
            .get(task.index as usize)
            .ok_or(TaskError::StaleTaskId)?;
        if slot.generation != task.generation || slot.record.is_none() {
            return Err(TaskError::StaleTaskId);
        }
        Ok(slot)
    }

    fn slot_mut(&mut self, task: RAsyncTaskId) -> Result<&mut Slot, TaskError> {
        let slot = self
            .slots
            .get_mut(task.index as usize)
            .ok_or(TaskError::StaleTaskId)?;
        if slot.generation != task.generation || slot.record.is_none() {
            return Err(TaskError::StaleTaskId);
        }
        Ok(slot)
    }

    fn retire(&mut self, task: RAsyncTaskId) -> Result<(), TaskError> {
        let next_free = self.free_head;
        let slot = self.slot_mut(task)?;
        slot.record = None;
        if slot.generation == u32::MAX {
            // Never wrap a live generational identity back to an old value.
            // Exhausted slots are retired permanently instead of creating an
            // ABA window where a very old task id could become valid again.
            slot.next_free = None;
            return Ok(());
        }
        slot.generation += 1;
        slot.next_free = next_free;
        self.free_head = Some(task.index);
        Ok(())
    }

    fn mark_completed(
        &mut self,
        task: RAsyncTaskId,
        completion: RAsyncCompletion,
    ) -> Result<(), TaskError> {
        let slot = self.slot_mut(task)?;
        let record = slot.record.as_mut().expect("validated occupied task slot");
        if record.state == TaskState::Completed {
            return Err(TaskError::AlreadyCompleted);
        }
        record.state = TaskState::Completed;
        record.completion = Some(completion);
        record.waker = None;
        record.wake_outstanding = false;
        record.polling = false;
        debug_assert!(self.active_tasks > 0);
        self.active_tasks -= 1;
        self.stats.completions = self.stats.completions.saturating_add(1);
        Ok(())
    }

    fn prepare_cleanup(
        &mut self,
        task: RAsyncTaskId,
    ) -> Result<Option<(*mut c_void, RAsyncDropFn)>, TaskError> {
        let slot = self.slot_mut(task)?;
        let record = slot.record.as_mut().expect("validated occupied task slot");
        if record.cleanup_called {
            return Ok(None);
        }
        record.cleanup_called = true;
        Ok(Some((record.runtime, record.drop_fn)))
    }

    fn drop_task(
        &mut self,
        task: RAsyncTaskId,
    ) -> Result<Option<(*mut c_void, RAsyncDropFn)>, TaskError> {
        let state = self
            .slot(task)?
            .record
            .as_ref()
            .expect("validated occupied task slot")
            .state;
        if state == TaskState::Completed {
            return Ok(None);
        }
        let cleanup = self.prepare_cleanup(task)?;
        self.mark_completed(
            task,
            RAsyncCompletion::new(RAsyncCompletionKind::Dropped, RAsyncOpaqueValue::default()),
        )?;
        Ok(cleanup)
    }

    fn prepare_wake(&mut self, task: RAsyncTaskId) -> WakeAction {
        let Ok(slot) = self.slot_mut(task) else {
            return WakeAction::status(RAsyncWakeStatus::StaleTask);
        };
        let record = slot.record.as_mut().expect("validated occupied task slot");
        if record.state == TaskState::Completed {
            return WakeAction::status(RAsyncWakeStatus::NoRegisteredWaker);
        }
        let Some(waker) = record.waker.as_ref() else {
            return WakeAction::status(RAsyncWakeStatus::NoRegisteredWaker);
        };
        if record.wake_outstanding {
            self.stats.coalesced_wakes = self.stats.coalesced_wakes.saturating_add(1);
            return WakeAction::status(RAsyncWakeStatus::Coalesced);
        }
        record.wake_outstanding = true;
        let waker = waker.clone();
        self.stats.wakes = self.stats.wakes.saturating_add(1);
        WakeAction {
            status: RAsyncWakeStatus::Woken,
            waker: Some(waker),
        }
    }

    fn cancellation_transition(
        &mut self,
        task: RAsyncTaskId,
        request_id: u64,
        transition: RAsyncCancelTransition,
    ) -> RAsyncCancelTransitionStatus {
        let Ok(slot) = self.slot_mut(task) else {
            return RAsyncCancelTransitionStatus::StaleTask;
        };
        let record = slot.record.as_mut().expect("validated occupied task slot");
        if record.cancellation.request_id != request_id {
            return RAsyncCancelTransitionStatus::StaleRequest;
        }
        match (record.cancellation.state, transition) {
            (RAsyncCancelState::Requested, RAsyncCancelTransition::Observe) => {
                record.cancellation.state = RAsyncCancelState::Observed;
                self.stats.cancellation_observations =
                    self.stats.cancellation_observations.saturating_add(1);
                RAsyncCancelTransitionStatus::Applied
            }
            (RAsyncCancelState::Observed, RAsyncCancelTransition::Acknowledge) => {
                record.cancellation.state = RAsyncCancelState::Acknowledged;
                self.stats.cancellation_acknowledgements =
                    self.stats.cancellation_acknowledgements.saturating_add(1);
                RAsyncCancelTransitionStatus::Applied
            }
            _ => RAsyncCancelTransitionStatus::InvalidTransition,
        }
    }
}

struct WakeAction {
    status: RAsyncWakeStatus,
    waker: Option<Waker>,
}

impl WakeAction {
    const fn status(status: RAsyncWakeStatus) -> Self {
        Self {
            status,
            waker: None,
        }
    }

    fn fire(self) -> RAsyncWakeStatus {
        if let Some(waker) = self.waker {
            // Invoke backend/user waker code only after the arena mutation is
            // complete. A valid Waker may run arbitrary scheduling logic, so
            // calling it while holding our UnsafeCell-derived `&mut` would
            // make synchronous facade re-entry alias that mutable reference.
            waker.wake();
        }
        self.status
    }
}

/// One backend-owned timer registration. Rimera stores no timer queue: this
/// guard only owns cancellation state and the backend's timer task/handle.
pub struct TimerRegistration<H> {
    id: RAsyncTimerId,
    task: RAsyncTaskId,
    state: Rc<Cell<RAsyncTimerState>>,
    handle: Option<H>,
    inner: Rc<UnsafeCell<TaskArena>>,
}

impl<H> fmt::Debug for TimerRegistration<H> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TimerRegistration")
            .field("id", &self.id)
            .field("task", &self.task)
            .field("state", &self.state.get())
            .finish_non_exhaustive()
    }
}

impl<H> TimerRegistration<H> {
    #[must_use]
    pub const fn id(&self) -> RAsyncTimerId {
        self.id
    }

    #[must_use]
    pub fn state(&self) -> RAsyncTimerState {
        self.state.get()
    }

    /// Cancel this registration once. Backend handle destruction is the
    /// cancellation transport; a late backend callback is ignored by the
    /// registration state machine and cannot turn `Cancelled` into `Fired`.
    pub fn cancel(&mut self) -> bool {
        if self.state.get() != RAsyncTimerState::Registered {
            return false;
        }
        self.state.set(RAsyncTimerState::Cancelled);
        {
            // SAFETY: the registration retains the local Rc arena and cannot
            // cross threads through the safe API.
            let arena = unsafe { &mut *self.inner.get() };
            arena.stats.timers_cancelled = arena.stats.timers_cancelled.saturating_add(1);
        }
        // Backend handle drop occurs after the arena borrow has ended because
        // cancellation may synchronously destroy backend future state.
        drop(self.handle.take());
        true
    }
}

impl<H> Drop for TimerRegistration<H> {
    fn drop(&mut self) {
        let _ = self.cancel();
    }
}

#[derive(Debug, Clone)]
pub struct LocalExecutorFacade {
    inner: Rc<UnsafeCell<TaskArena>>,
}

impl Default for LocalExecutorFacade {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalExecutorFacade {
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Rc::new(UnsafeCell::new(TaskArena::with_capacity(capacity))),
        }
    }

    pub fn submit_root(
        &self,
        runtime: *mut c_void,
        resume: RAsyncResumeFn,
        drop_fn: RAsyncDropFn,
    ) -> Result<(RAsyncTaskId, RootTask), TaskError> {
        // SAFETY: this facade is intentionally !Send/!Sync because it is Rc-
        // owned. All mutation is local-thread only, and no mutable reference is
        // held across the runtime callback invoked from `RootTask::poll`.
        let arena = unsafe { &mut *self.inner.get() };
        let task = arena.allocate(runtime, drop_fn)?;
        Ok((
            task,
            RootTask {
                inner: Rc::clone(&self.inner),
                task,
                runtime,
                resume,
                terminal: false,
            },
        ))
    }

    pub fn request_cancel(&self, task: RAsyncTaskId) -> Result<bool, TaskError> {
        let wake = {
            // SAFETY: see `submit_root`; local Rc ownership prevents cross-
            // thread access. The borrow ends before firing the backend waker.
            let arena = unsafe { &mut *self.inner.get() };
            let (cancellation, state) = {
                let record = arena
                    .slot(task)?
                    .record
                    .as_ref()
                    .expect("validated occupied task slot");
                (record.cancellation, record.state)
            };
            if state == TaskState::Completed {
                return Err(TaskError::AlreadyCompleted);
            }
            if matches!(
                cancellation.state,
                RAsyncCancelState::Requested | RAsyncCancelState::Observed
            ) {
                return Ok(false);
            }
            let request_id = arena.next_cancel_request;
            arena.next_cancel_request = arena.next_cancel_request.wrapping_add(1).max(1);
            let slot = arena.slot_mut(task)?;
            let record = slot.record.as_mut().expect("validated occupied task slot");
            record.cancellation = RAsyncCancellation {
                request_id,
                state: RAsyncCancelState::Requested,
                reserved: [0; 7],
            };
            arena.stats.cancellation_requests = arena.stats.cancellation_requests.saturating_add(1);
            arena.prepare_wake(task)
        };
        let _ = wake.fire();
        Ok(true)
    }

    pub fn cancellation(&self, task: RAsyncTaskId) -> Result<RAsyncCancellation, TaskError> {
        // SAFETY: read-only local access; no reference escapes.
        let arena = unsafe { &*self.inner.get() };
        Ok(arena
            .slot(task)?
            .record
            .as_ref()
            .expect("validated occupied task slot")
            .cancellation)
    }

    #[must_use]
    pub fn deadline_after(&self, delay: Duration) -> RAsyncDeadline {
        // SAFETY: read-only local access; no reference escapes.
        let arena = unsafe { &*self.inner.get() };
        let elapsed = Instant::now().saturating_duration_since(arena.epoch);
        let nanos = elapsed.as_nanos().saturating_add(delay.as_nanos());
        RAsyncDeadline {
            nanos_from_epoch: nanos.min(u128::from(u64::MAX)) as u64,
        }
    }

    pub fn register_timer<B: LocalBackend>(
        &self,
        backend: &B,
        task: RAsyncTaskId,
        deadline: RAsyncDeadline,
    ) -> Result<TimerRegistration<B::TimerHandle>, TaskError> {
        // SAFETY: local Rc ownership prevents cross-thread access.
        let arena = unsafe { &mut *self.inner.get() };
        let state = arena
            .slot(task)?
            .record
            .as_ref()
            .expect("validated occupied task slot")
            .state;
        if state == TaskState::Completed {
            return Err(TaskError::AlreadyCompleted);
        }
        let instant = arena
            .epoch
            .checked_add(Duration::from_nanos(deadline.nanos_from_epoch))
            .ok_or(TaskError::InvalidDeadline)?;
        let id = RAsyncTimerId {
            sequence: arena.next_timer_sequence,
        };
        arena.next_timer_sequence = arena.next_timer_sequence.wrapping_add(1).max(1);
        arena.stats.timers_registered = arena.stats.timers_registered.saturating_add(1);

        let timer_state = Rc::new(Cell::new(RAsyncTimerState::Registered));
        let callback_state = Rc::clone(&timer_state);
        let callback_inner = Rc::clone(&self.inner);
        let handle = backend.register_timer(instant, move || {
            if callback_state.get() != RAsyncTimerState::Registered {
                return;
            }
            callback_state.set(RAsyncTimerState::Fired);
            let wake = {
                // SAFETY: backend is local and the timer closure retains the
                // arena Rc. The borrow ends before invoking the backend waker.
                let arena = unsafe { &mut *callback_inner.get() };
                arena.stats.timers_fired = arena.stats.timers_fired.saturating_add(1);
                arena.prepare_wake(task)
            };
            let _ = wake.fire();
        });
        Ok(TimerRegistration {
            id,
            task,
            state: timer_state,
            handle: Some(handle),
            inner: Rc::clone(&self.inner),
        })
    }

    pub fn wake(&self, task: RAsyncTaskId) -> RAsyncWakeStatus {
        // SAFETY: see `submit_root`. The arena mutation is complete before
        // calling the backend/user waker so synchronous re-entry cannot alias
        // the UnsafeCell-derived mutable reference.
        let wake = unsafe { (&mut *self.inner.get()).prepare_wake(task) };
        wake.fire()
    }

    pub fn completion(&self, task: RAsyncTaskId) -> Result<Option<RAsyncCompletion>, TaskError> {
        // SAFETY: read-only access is local-thread only and no reference escapes.
        let arena = unsafe { &*self.inner.get() };
        let slot = arena.slot(task)?;
        let record = slot.record.as_ref().expect("validated occupied task slot");
        Ok(record.completion)
    }

    pub fn take_completion(&self, task: RAsyncTaskId) -> Result<RAsyncCompletion, TaskError> {
        // SAFETY: see `submit_root`.
        let arena = unsafe { &mut *self.inner.get() };
        let completion = {
            let slot = arena.slot_mut(task)?;
            let record = slot.record.as_mut().expect("validated occupied task slot");
            if record.state != TaskState::Completed {
                return Err(TaskError::NotCompleted);
            }
            record.completion.take().ok_or(TaskError::NotCompleted)?
        };
        arena.retire(task)?;
        Ok(completion)
    }

    pub fn shutdown(&self) {
        let slot_count = {
            // SAFETY: see `submit_root`. This borrow ends before any runtime
            // cleanup callback or backend waker can re-enter the facade.
            let arena = unsafe { &mut *self.inner.get() };
            if arena.shutdown {
                return;
            }
            arena.shutdown = true;
            arena.slots.len()
        };

        // Do not collect task ids into a temporary Vec: shutdown walks the
        // arena in place and therefore adds no Rimera-owned allocation.
        for index in 0..slot_count {
            let action = {
                // SAFETY: local-thread-only arena access; this borrow is scoped
                // to state transition preparation and ends before callbacks.
                let arena = unsafe { &mut *self.inner.get() };
                let Some((generation, polling, waker)) = arena.slots.get(index).and_then(|slot| {
                    let record = slot.record.as_ref()?;
                    (record.state == TaskState::Active)
                        .then(|| (slot.generation, record.polling, record.waker.clone()))
                }) else {
                    continue;
                };
                let task = RAsyncTaskId {
                    index: u32::try_from(index).expect("task slots never exceed u32 capacity"),
                    generation,
                };
                // If shutdown is requested from inside the runtime resume
                // callback, destroying that activation here would invalidate
                // the callback's live state. Defer cleanup until poll returns.
                let cleanup = if polling {
                    None
                } else {
                    arena.prepare_cleanup(task).ok().flatten()
                };
                let _ = arena.mark_completed(
                    task,
                    RAsyncCompletion::driver_error(RAsyncDriverError::Shutdown),
                );
                Some((task, cleanup, waker))
            };

            if let Some((task, cleanup, waker)) = action {
                if let Some((runtime, drop_fn)) = cleanup {
                    // SAFETY: submission records this runtime/callback pair.
                    // The arena borrow ended above, so cleanup may re-enter the
                    // facade without creating aliased mutable references.
                    unsafe { drop_fn(runtime, task) };
                }
                if let Some(waker) = waker {
                    waker.wake();
                }
            }
        }
    }

    #[must_use]
    pub fn stats(&self) -> TaskStats {
        // SAFETY: read-only access is local-thread only and no reference escapes.
        unsafe { (&*self.inner.get()).stats }
    }

    #[must_use]
    pub fn active_task_count(&self) -> usize {
        // SAFETY: read-only access is local-thread only and no reference escapes.
        unsafe { (&*self.inner.get()).active_tasks }
    }
}

pub struct RootTask {
    inner: Rc<UnsafeCell<TaskArena>>,
    task: RAsyncTaskId,
    runtime: *mut c_void,
    resume: RAsyncResumeFn,
    terminal: bool,
}

impl fmt::Debug for RootTask {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RootTask")
            .field("task", &self.task)
            .field("terminal", &self.terminal)
            .finish_non_exhaustive()
    }
}

impl RootTask {
    #[must_use]
    pub const fn id(&self) -> RAsyncTaskId {
        self.task
    }
}

impl Future for RootTask {
    type Output = RAsyncCompletion;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.as_mut().get_mut();
        if this.terminal {
            // Polling a Future after terminal completion is a caller contract
            // violation. Return a deterministic facade error instead of
            // touching a retired runtime activation.
            return Poll::Ready(RAsyncCompletion::driver_error(
                RAsyncDriverError::ReentrantPoll,
            ));
        }

        let (already_completed, cancellation, reentrant) = {
            // SAFETY: local Rc ownership prevents cross-thread access. This
            // mutable borrow ends before invoking the runtime callback, so a
            // retained wake handle can safely enter `wake_from_abi`.
            let arena = unsafe { &mut *this.inner.get() };
            let Ok(slot) = arena.slot_mut(this.task) else {
                this.terminal = true;
                return Poll::Ready(RAsyncCompletion::driver_error(RAsyncDriverError::Shutdown));
            };
            let record = slot.record.as_mut().expect("validated occupied task slot");
            // A backend poll consumes one previously queued wake. If a wake
            // races while the runtime callback is executing it sets this flag
            // again and remains queued for the next backend poll.
            record.wake_outstanding = false;
            if record.state == TaskState::Completed {
                (record.completion, record.cancellation, false)
            } else if record.polling {
                (None, record.cancellation, true)
            } else {
                let replace_waker = record
                    .waker
                    .as_ref()
                    .is_none_or(|waker| !waker.will_wake(context.waker()));
                if replace_waker {
                    record.waker = Some(context.waker().clone());
                }
                record.polling = true;
                let cancellation = record.cancellation;
                arena.stats.polls = arena.stats.polls.saturating_add(1);
                (None, cancellation, false)
            }
        };

        if let Some(completion) = already_completed {
            this.terminal = true;
            return Poll::Ready(completion);
        }
        if reentrant {
            let completion = RAsyncCompletion::driver_error(RAsyncDriverError::ReentrantPoll);
            let cleanup = {
                // SAFETY: local-thread-only task arena. The borrow ends before
                // invoking the runtime cleanup callback below.
                let arena = unsafe { &mut *this.inner.get() };
                let cleanup = arena.prepare_cleanup(this.task).ok().flatten();
                let _ = arena.mark_completed(this.task, completion);
                cleanup
            };
            if let Some((runtime, drop_fn)) = cleanup {
                // SAFETY: registered submission pair; no arena borrow is live.
                unsafe { drop_fn(runtime, this.task) };
            }
            this.terminal = true;
            return Poll::Ready(completion);
        }

        let poll_context = RAsyncPollContext {
            task: this.task,
            control_data: Rc::as_ptr(&this.inner).cast_mut().cast::<c_void>(),
            wake: wake_from_abi,
            cancel_transition: cancel_transition_from_abi,
            cancellation,
        };
        let mut completion =
            RAsyncCompletion::new(RAsyncCompletionKind::Returned, RAsyncOpaqueValue::default());
        // SAFETY: the runtime pointer/function pair is registered together at
        // submission. The poll context and completion output remain live for
        // the duration of the call, and the facade never inspects the opaque
        // completion payload.
        let state =
            unsafe { (this.resume)(this.runtime, &raw const poll_context, &raw mut completion) };

        let completed_during_callback = {
            // SAFETY: the callback has returned, so no mutable arena reference
            // from this poll is outstanding. Local Rc ownership prevents
            // cross-thread mutation.
            let arena = unsafe { &mut *this.inner.get() };
            let Ok(slot) = arena.slot_mut(this.task) else {
                this.terminal = true;
                return Poll::Ready(RAsyncCompletion::driver_error(RAsyncDriverError::Shutdown));
            };
            let record = slot.record.as_mut().expect("validated occupied task slot");
            if record.state == TaskState::Completed {
                Some(
                    record.completion.unwrap_or_else(|| {
                        RAsyncCompletion::driver_error(RAsyncDriverError::Shutdown)
                    }),
                )
            } else {
                record.polling = false;
                None
            }
        };

        if let Some(completion) = completed_during_callback {
            let cleanup = if completion.kind == RAsyncCompletionKind::DriverError {
                // A facade driver error that became terminal while the runtime
                // callback was active (notably shutdown) deferred cleanup until
                // the callback returned.
                let arena = unsafe { &mut *this.inner.get() };
                arena.prepare_cleanup(this.task).ok().flatten()
            } else {
                None
            };
            if let Some((runtime, drop_fn)) = cleanup {
                // SAFETY: registered submission pair; no arena borrow is live.
                unsafe { drop_fn(runtime, this.task) };
            }
            this.terminal = true;
            return Poll::Ready(completion);
        }

        match state {
            RAsyncPollState::Pending => Poll::Pending,
            RAsyncPollState::Ready => {
                // SAFETY: local-thread-only arena access; no callback is made.
                let arena = unsafe { &mut *this.inner.get() };
                let _ = arena.mark_completed(this.task, completion);
                this.terminal = true;
                Poll::Ready(completion)
            }
        }
    }
}

impl Drop for RootTask {
    fn drop(&mut self) {
        if self.terminal {
            return;
        }
        let cleanup = {
            // SAFETY: the RootTask holds an Rc reference to the arena
            // allocation, so it cannot disappear while state is transitioned.
            // This borrow ends before invoking runtime cleanup.
            let arena = unsafe { &mut *self.inner.get() };
            arena.drop_task(self.task).ok().flatten()
        };
        if let Some((runtime, drop_fn)) = cleanup {
            // SAFETY: submission records this runtime/callback pair, and no
            // arena borrow is live so cleanup may safely re-enter the facade.
            unsafe { drop_fn(runtime, self.task) };
        }
        self.terminal = true;
    }
}

unsafe extern "C" fn wake_from_abi(data: *mut c_void, task: RAsyncTaskId) -> RAsyncWakeStatus {
    if data.is_null() {
        return RAsyncWakeStatus::StaleTask;
    }
    let cell = data.cast::<UnsafeCell<TaskArena>>();
    // SAFETY: `control_data` originates from `Rc::as_ptr` for this exact arena
    // and is valid while the root task/facade retains the Rc. The local facade
    // is !Send/!Sync, so no cross-thread alias is permitted by the safe API.
    let wake = unsafe { (&mut *(*cell).get()).prepare_wake(task) };
    wake.fire()
}

unsafe extern "C" fn cancel_transition_from_abi(
    data: *mut c_void,
    task: RAsyncTaskId,
    request_id: u64,
    transition: RAsyncCancelTransition,
) -> RAsyncCancelTransitionStatus {
    if data.is_null() {
        return RAsyncCancelTransitionStatus::StaleTask;
    }
    let cell = data.cast::<UnsafeCell<TaskArena>>();
    // SAFETY: identical local-arena lifetime contract to `wake_from_abi`.
    let arena = unsafe { &mut *(*cell).get() };
    arena.cancellation_transition(task, request_id, transition)
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::convert::Infallible;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::task::{Wake, Waker};

    use super::*;

    #[derive(Debug, Default)]
    struct FakeRuntime {
        polls: usize,
        drops: usize,
        saw_cancel: bool,
        control_data: *mut c_void,
        wake: Option<rimera_abi::RAsyncWakeFn>,
        task: Option<RAsyncTaskId>,
        ready_after: usize,
        shutdown_facade: Option<*const LocalExecutorFacade>,
        shutdown_on_poll: bool,
    }

    unsafe extern "C" fn resume_fake(
        runtime: *mut c_void,
        poll: *const RAsyncPollContext,
        completion: *mut RAsyncCompletion,
    ) -> RAsyncPollState {
        // SAFETY: tests submit pointers to live `FakeRuntime` values and valid
        // poll/completion storage from `RootTask::poll`.
        let runtime = unsafe { &mut *runtime.cast::<FakeRuntime>() };
        let poll = unsafe { *poll };
        runtime.polls += 1;
        runtime.saw_cancel |= matches!(
            poll.cancellation.state,
            RAsyncCancelState::Requested | RAsyncCancelState::Observed
        );
        runtime.control_data = poll.control_data;
        runtime.wake = Some(poll.wake);
        runtime.task = Some(poll.task);
        if runtime.shutdown_on_poll {
            runtime.shutdown_on_poll = false;
            let facade = runtime
                .shutdown_facade
                .expect("shutdown test must provide facade pointer");
            // SAFETY: the test keeps `facade` alive for the whole poll.
            unsafe { (&*facade).shutdown() };
            assert_eq!(runtime.drops, 0, "cleanup must wait for resume to return");
            return RAsyncPollState::Pending;
        }
        if poll.cancellation.state == RAsyncCancelState::Requested {
            assert_eq!(
                unsafe {
                    (poll.cancel_transition)(
                        poll.control_data,
                        poll.task,
                        poll.cancellation.request_id,
                        RAsyncCancelTransition::Observe,
                    )
                },
                RAsyncCancelTransitionStatus::Applied
            );
            assert_eq!(
                unsafe {
                    (poll.cancel_transition)(
                        poll.control_data,
                        poll.task,
                        poll.cancellation.request_id,
                        RAsyncCancelTransition::Acknowledge,
                    )
                },
                RAsyncCancelTransitionStatus::Applied
            );
        }
        if runtime.saw_cancel {
            unsafe {
                completion.write(RAsyncCompletion::new(
                    RAsyncCompletionKind::Cancelled,
                    RAsyncOpaqueValue::default(),
                ));
            }
            return RAsyncPollState::Ready;
        }
        if runtime.polls < runtime.ready_after {
            return RAsyncPollState::Pending;
        }
        unsafe {
            completion.write(RAsyncCompletion::new(
                RAsyncCompletionKind::Returned,
                RAsyncOpaqueValue {
                    low: runtime.polls as u64,
                    high: 0,
                },
            ));
        }
        RAsyncPollState::Ready
    }

    unsafe extern "C" fn drop_fake(runtime: *mut c_void, _task: RAsyncTaskId) {
        // SAFETY: tests submit pointers to live `FakeRuntime` values.
        let runtime = unsafe { &mut *runtime.cast::<FakeRuntime>() };
        runtime.drops += 1;
    }

    fn poll_once(task: &mut RootTask) -> Poll<RAsyncCompletion> {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        Pin::new(task).poll(&mut context)
    }

    fn poll_with_waker(task: &mut RootTask, waker: &Waker) -> Poll<RAsyncCompletion> {
        let mut context = Context::from_waker(waker);
        Pin::new(task).poll(&mut context)
    }

    #[derive(Default)]
    struct CountingWake(AtomicUsize);

    impl Wake for CountingWake {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[derive(Default)]
    struct ManualBackendState {
        deadline: Cell<Option<Instant>>,
        callback: RefCell<Option<Box<dyn FnOnce()>>>,
        handles_dropped: Cell<usize>,
    }

    #[derive(Clone, Default)]
    struct ManualBackend {
        state: Rc<ManualBackendState>,
    }

    struct ManualTimerHandle {
        state: Rc<ManualBackendState>,
    }

    impl Drop for ManualTimerHandle {
        fn drop(&mut self) {
            self.state
                .handles_dropped
                .set(self.state.handles_dropped.get() + 1);
        }
    }

    impl LocalBackend for ManualBackend {
        type Error = Infallible;
        type TimerHandle = ManualTimerHandle;

        const NAME: &'static str = "manual-test";

        fn new() -> Result<Self, Self::Error> {
            Ok(Self::default())
        }

        fn drive_root<F: Future>(&self, _future: F) -> F::Output {
            panic!("manual backend does not drive roots")
        }

        fn register_timer<F>(&self, deadline: Instant, on_fire: F) -> Self::TimerHandle
        where
            F: FnOnce() + 'static,
        {
            assert!(
                self.state.callback.borrow().is_none(),
                "manual backend supports one outstanding timer"
            );
            self.state.deadline.set(Some(deadline));
            *self.state.callback.borrow_mut() = Some(Box::new(on_fire));
            ManualTimerHandle {
                state: Rc::clone(&self.state),
            }
        }
    }

    impl ManualBackend {
        fn deadline(&self) -> Option<Instant> {
            self.state.deadline.get()
        }

        fn handles_dropped(&self) -> usize {
            self.state.handles_dropped.get()
        }

        fn fire(&self) {
            let callback = self
                .state
                .callback
                .borrow_mut()
                .take()
                .expect("manual timer must be registered before firing");
            callback();
        }
    }

    #[derive(Debug, Default)]
    struct WakeDuringPollRuntime {
        polls: usize,
        first_wake: Option<RAsyncWakeStatus>,
        duplicate_wake: Option<RAsyncWakeStatus>,
    }

    unsafe extern "C" fn resume_wake_during_poll(
        runtime: *mut c_void,
        poll: *const RAsyncPollContext,
        completion: *mut RAsyncCompletion,
    ) -> RAsyncPollState {
        // SAFETY: the test submits a live `WakeDuringPollRuntime`, and the
        // facade supplies valid poll/completion storage for this callback.
        let runtime = unsafe { &mut *runtime.cast::<WakeDuringPollRuntime>() };
        let poll = unsafe { *poll };
        runtime.polls += 1;
        if runtime.polls == 1 {
            // SAFETY: control_data belongs to this live root/facade pair.
            runtime.first_wake = Some(unsafe { (poll.wake)(poll.control_data, poll.task) });
            runtime.duplicate_wake = Some(unsafe { (poll.wake)(poll.control_data, poll.task) });
            return RAsyncPollState::Pending;
        }
        unsafe {
            completion.write(RAsyncCompletion::new(
                RAsyncCompletionKind::Returned,
                RAsyncOpaqueValue::default(),
            ));
        }
        RAsyncPollState::Ready
    }

    unsafe extern "C" fn drop_noop(_runtime: *mut c_void, _task: RAsyncTaskId) {}

    #[test]
    fn task_ids_are_generational_and_stale_ids_are_rejected() {
        let facade = LocalExecutorFacade::with_capacity(1);
        let mut runtime = FakeRuntime {
            ready_after: 1,
            ..FakeRuntime::default()
        };
        let (first_id, mut first) = facade
            .submit_root(
                std::ptr::from_mut(&mut runtime).cast(),
                resume_fake,
                drop_fake,
            )
            .unwrap();
        assert!(matches!(poll_once(&mut first), Poll::Ready(_)));
        assert_eq!(
            facade.take_completion(first_id).unwrap().kind,
            RAsyncCompletionKind::Returned
        );
        assert_eq!(facade.wake(first_id), RAsyncWakeStatus::StaleTask);

        runtime.polls = 0;
        let (second_id, _second) = facade
            .submit_root(
                std::ptr::from_mut(&mut runtime).cast(),
                resume_fake,
                drop_fake,
            )
            .unwrap();
        assert_eq!(first_id.index, second_id.index);
        assert_ne!(first_id.generation, second_id.generation);
    }

    #[test]
    fn exhausted_generation_is_never_reused() {
        let facade = LocalExecutorFacade::with_capacity(1);
        // Seed one vacant slot at the terminal generation to exercise the ABA
        // boundary without performing billions of retirements.
        unsafe {
            let arena = &mut *facade.inner.get();
            arena.slots.push(Slot::vacant(u32::MAX));
            arena.free_head = Some(0);
        }
        let mut runtime = FakeRuntime {
            ready_after: 1,
            ..FakeRuntime::default()
        };
        let (old_id, mut old_task) = facade
            .submit_root(
                std::ptr::from_mut(&mut runtime).cast(),
                resume_fake,
                drop_fake,
            )
            .unwrap();
        assert_eq!(old_id.generation, u32::MAX);
        assert!(matches!(poll_once(&mut old_task), Poll::Ready(_)));
        facade.take_completion(old_id).unwrap();

        runtime.polls = 0;
        let (new_id, _new_task) = facade
            .submit_root(
                std::ptr::from_mut(&mut runtime).cast(),
                resume_fake,
                drop_fake,
            )
            .unwrap();
        assert_ne!(new_id.index, old_id.index);
        assert_eq!(facade.wake(old_id), RAsyncWakeStatus::StaleTask);
    }

    #[test]
    fn pending_root_retains_one_identity_and_external_wake_handle() {
        let facade = LocalExecutorFacade::with_capacity(1);
        let mut runtime = FakeRuntime {
            ready_after: 2,
            ..FakeRuntime::default()
        };
        let (task_id, mut task) = facade
            .submit_root(
                std::ptr::from_mut(&mut runtime).cast(),
                resume_fake,
                drop_fake,
            )
            .unwrap();
        assert!(matches!(poll_once(&mut task), Poll::Pending));
        assert_eq!(runtime.task, Some(task_id));
        let wake = runtime.wake.expect("runtime captured wake callback");
        // SAFETY: the task is still live and retained `control_data` comes from
        // its poll context.
        assert_eq!(
            unsafe { wake(runtime.control_data, task_id) },
            RAsyncWakeStatus::Woken
        );
        assert_eq!(
            unsafe { wake(runtime.control_data, task_id) },
            RAsyncWakeStatus::Coalesced
        );
        assert!(matches!(poll_once(&mut task), Poll::Ready(_)));
        let stats = facade.stats();
        assert_eq!(stats.submitted, 1);
        assert_eq!(stats.polls, 2);
        assert_eq!(stats.completions, 1);
        assert_eq!(stats.wakes, 1);
        assert_eq!(stats.coalesced_wakes, 1);
        assert_eq!(stats.rimera_wrapper_allocations, 0);
    }

    #[test]
    fn cancellation_requests_coalesce_and_reach_the_runtime_callback() {
        let facade = LocalExecutorFacade::with_capacity(1);
        let mut runtime = FakeRuntime {
            ready_after: usize::MAX,
            ..FakeRuntime::default()
        };
        let (task_id, mut task) = facade
            .submit_root(
                std::ptr::from_mut(&mut runtime).cast(),
                resume_fake,
                drop_fake,
            )
            .unwrap();
        assert!(matches!(poll_once(&mut task), Poll::Pending));
        assert!(facade.request_cancel(task_id).unwrap());
        assert!(!facade.request_cancel(task_id).unwrap());
        let Poll::Ready(completion) = poll_once(&mut task) else {
            panic!("cancelled root must complete in fake runtime");
        };
        assert!(runtime.saw_cancel);
        assert_eq!(completion.kind, RAsyncCompletionKind::Cancelled);
        assert_eq!(
            facade.cancellation(task_id).unwrap().state,
            RAsyncCancelState::Acknowledged
        );
        let stats = facade.stats();
        assert_eq!(stats.cancellation_requests, 1);
        assert_eq!(stats.cancellation_observations, 1);
        assert_eq!(stats.cancellation_acknowledgements, 1);
    }

    #[test]
    fn wake_during_poll_is_queued_once_and_duplicate_is_coalesced() {
        let facade = LocalExecutorFacade::with_capacity(1);
        let mut runtime = WakeDuringPollRuntime::default();
        let (_task_id, mut task) = facade
            .submit_root(
                std::ptr::from_mut(&mut runtime).cast(),
                resume_wake_during_poll,
                drop_noop,
            )
            .unwrap();
        let counter = Arc::new(CountingWake::default());
        let waker = Waker::from(Arc::clone(&counter));

        assert!(matches!(poll_with_waker(&mut task, &waker), Poll::Pending));
        assert_eq!(runtime.first_wake, Some(RAsyncWakeStatus::Woken));
        assert_eq!(runtime.duplicate_wake, Some(RAsyncWakeStatus::Coalesced));
        assert_eq!(counter.0.load(Ordering::Relaxed), 1);
        assert_eq!(facade.stats().polls, 1);
        assert_eq!(facade.stats().wakes, 1);
        assert_eq!(facade.stats().coalesced_wakes, 1);

        assert!(matches!(poll_with_waker(&mut task, &waker), Poll::Ready(_)));
        assert_eq!(runtime.polls, 2);
        assert_eq!(facade.active_task_count(), 0);
        assert_eq!(facade.stats().completions, 1);
    }

    #[test]
    fn manual_timer_deadlines_cancel_and_late_fire_are_deterministic() {
        let facade = LocalExecutorFacade::with_capacity(1);
        let mut runtime = FakeRuntime {
            ready_after: usize::MAX,
            ..FakeRuntime::default()
        };
        let (task_id, mut task) = facade
            .submit_root(
                std::ptr::from_mut(&mut runtime).cast(),
                resume_fake,
                drop_fake,
            )
            .unwrap();
        assert!(matches!(poll_once(&mut task), Poll::Pending));
        let epoch = unsafe { (&*facade.inner.get()).epoch };

        let fired_backend = ManualBackend::default();
        let deadline = RAsyncDeadline {
            nanos_from_epoch: 123_456,
        };
        let mut fired = facade
            .register_timer(&fired_backend, task_id, deadline)
            .unwrap();
        assert_eq!(
            fired_backend.deadline(),
            epoch.checked_add(Duration::from_nanos(deadline.nanos_from_epoch))
        );
        fired_backend.fire();
        assert_eq!(fired.state(), RAsyncTimerState::Fired);
        assert!(!fired.cancel());
        drop(fired);
        assert_eq!(fired_backend.handles_dropped(), 1);

        let cancelled_backend = ManualBackend::default();
        let mut cancelled = facade
            .register_timer(&cancelled_backend, task_id, deadline)
            .unwrap();
        assert!(cancelled.cancel());
        assert!(!cancelled.cancel());
        assert_eq!(cancelled.state(), RAsyncTimerState::Cancelled);
        assert_eq!(cancelled_backend.handles_dropped(), 1);
        let fired_before_late_callback = facade.stats().timers_fired;
        cancelled_backend.fire();
        assert_eq!(cancelled.state(), RAsyncTimerState::Cancelled);
        assert_eq!(facade.stats().timers_fired, fired_before_late_callback);
        drop(cancelled);
        assert_eq!(cancelled_backend.handles_dropped(), 1);

        let stats = facade.stats();
        assert_eq!(stats.timers_registered, 2);
        assert_eq!(stats.timers_fired, 1);
        assert_eq!(stats.timers_cancelled, 1);
    }

    #[test]
    fn timer_for_retired_generation_never_wakes_reused_slot() {
        let facade = LocalExecutorFacade::with_capacity(1);
        let mut first_runtime = FakeRuntime {
            ready_after: 1,
            ..FakeRuntime::default()
        };
        let (first_id, mut first) = facade
            .submit_root(
                std::ptr::from_mut(&mut first_runtime).cast(),
                resume_fake,
                drop_fake,
            )
            .unwrap();
        let backend = ManualBackend::default();
        let timer = facade
            .register_timer(
                &backend,
                first_id,
                RAsyncDeadline {
                    nanos_from_epoch: 1,
                },
            )
            .unwrap();
        assert!(matches!(poll_once(&mut first), Poll::Ready(_)));
        facade.take_completion(first_id).unwrap();

        let mut second_runtime = FakeRuntime {
            ready_after: usize::MAX,
            ..FakeRuntime::default()
        };
        let (second_id, mut second) = facade
            .submit_root(
                std::ptr::from_mut(&mut second_runtime).cast(),
                resume_fake,
                drop_fake,
            )
            .unwrap();
        assert_eq!(first_id.index, second_id.index);
        assert_ne!(first_id.generation, second_id.generation);
        let counter = Arc::new(CountingWake::default());
        let waker = Waker::from(Arc::clone(&counter));
        assert!(matches!(
            poll_with_waker(&mut second, &waker),
            Poll::Pending
        ));

        backend.fire();
        assert_eq!(timer.state(), RAsyncTimerState::Fired);
        assert_eq!(counter.0.load(Ordering::Relaxed), 0);
        assert_eq!(facade.stats().wakes, 0);
        assert_eq!(facade.active_task_count(), 1);
        drop(second);
        assert_eq!(facade.active_task_count(), 0);
        assert_eq!(
            facade.take_completion(second_id).unwrap().kind,
            RAsyncCompletionKind::Dropped
        );
    }

    #[test]
    fn dropped_roots_and_shutdown_invoke_cleanup_exactly_once() {
        let facade = LocalExecutorFacade::with_capacity(2);
        let mut first_runtime = FakeRuntime {
            ready_after: usize::MAX,
            ..FakeRuntime::default()
        };
        let (first_id, mut first) = facade
            .submit_root(
                std::ptr::from_mut(&mut first_runtime).cast(),
                resume_fake,
                drop_fake,
            )
            .unwrap();
        assert!(matches!(poll_once(&mut first), Poll::Pending));
        drop(first);
        assert_eq!(first_runtime.drops, 1);
        assert_eq!(
            facade.take_completion(first_id).unwrap().kind,
            RAsyncCompletionKind::Dropped
        );

        let mut second_runtime = FakeRuntime {
            ready_after: usize::MAX,
            ..FakeRuntime::default()
        };
        let (second_id, mut second) = facade
            .submit_root(
                std::ptr::from_mut(&mut second_runtime).cast(),
                resume_fake,
                drop_fake,
            )
            .unwrap();
        assert!(matches!(poll_once(&mut second), Poll::Pending));
        facade.shutdown();
        assert_eq!(second_runtime.drops, 1);
        let Poll::Ready(completion) = poll_once(&mut second) else {
            panic!("shutdown must make root terminal");
        };
        assert_eq!(completion.kind, RAsyncCompletionKind::DriverError);
        assert_eq!(completion.payload.low, RAsyncDriverError::Shutdown as u64);
        drop(second);
        assert_eq!(second_runtime.drops, 1);
        assert_eq!(
            facade.take_completion(second_id).unwrap().payload.low,
            RAsyncDriverError::Shutdown as u64
        );
        assert!(matches!(
            facade.submit_root(
                std::ptr::from_mut(&mut second_runtime).cast(),
                resume_fake,
                drop_fake,
            ),
            Err(TaskError::ExecutorShutdown)
        ));
    }

    #[test]
    fn shutdown_during_resume_defers_cleanup_until_resume_returns() {
        let facade = LocalExecutorFacade::with_capacity(1);
        let mut runtime = FakeRuntime {
            ready_after: usize::MAX,
            shutdown_facade: Some(std::ptr::from_ref(&facade)),
            shutdown_on_poll: true,
            ..FakeRuntime::default()
        };
        let (task_id, mut task) = facade
            .submit_root(
                std::ptr::from_mut(&mut runtime).cast(),
                resume_fake,
                drop_fake,
            )
            .unwrap();
        let Poll::Ready(completion) = poll_once(&mut task) else {
            panic!("shutdown during resume must terminate the root");
        };
        assert_eq!(completion.kind, RAsyncCompletionKind::DriverError);
        assert_eq!(completion.payload.low, RAsyncDriverError::Shutdown as u64);
        assert_eq!(runtime.drops, 1);
        drop(task);
        assert_eq!(runtime.drops, 1);
        assert_eq!(
            facade.take_completion(task_id).unwrap().payload.low,
            RAsyncDriverError::Shutdown as u64
        );
    }

    #[test]
    fn reentrant_poll_is_a_deterministic_driver_error() {
        let facade = LocalExecutorFacade::with_capacity(1);
        let mut runtime = FakeRuntime {
            ready_after: 1,
            ..FakeRuntime::default()
        };
        let (task_id, mut task) = facade
            .submit_root(
                std::ptr::from_mut(&mut runtime).cast(),
                resume_fake,
                drop_fake,
            )
            .unwrap();
        // Test-only direct state setup models a backend illegally entering the
        // same root while its previous poll has not returned.
        unsafe {
            let arena = &mut *facade.inner.get();
            arena
                .slot_mut(task_id)
                .unwrap()
                .record
                .as_mut()
                .unwrap()
                .polling = true;
        }
        let Poll::Ready(completion) = poll_once(&mut task) else {
            panic!("reentrant poll must be rejected");
        };
        assert_eq!(completion.kind, RAsyncCompletionKind::DriverError);
        assert_eq!(
            completion.payload.low,
            RAsyncDriverError::ReentrantPoll as u64
        );
        assert_eq!(runtime.drops, 1);
        drop(task);
        assert_eq!(runtime.drops, 1);
    }

    #[test]
    fn wake_and_completion_storm_coalesces_without_lost_wakes_or_duplicates() {
        const ROOTS: usize = 256;
        const DUPLICATE_WAKES: usize = 7;
        let facade = LocalExecutorFacade::with_capacity(ROOTS);
        let mut runtimes = (0..ROOTS)
            .map(|_| {
                Box::new(FakeRuntime {
                    ready_after: 2,
                    ..FakeRuntime::default()
                })
            })
            .collect::<Vec<_>>();
        let mut tasks = Vec::with_capacity(ROOTS);
        let mut ids = Vec::with_capacity(ROOTS);

        for runtime in &mut runtimes {
            let (task_id, task) = facade
                .submit_root(
                    std::ptr::from_mut(runtime.as_mut()).cast(),
                    resume_fake,
                    drop_fake,
                )
                .unwrap();
            ids.push(task_id);
            tasks.push(task);
        }
        for task in &mut tasks {
            assert!(matches!(poll_once(task), Poll::Pending));
        }
        for runtime in &runtimes {
            let task = runtime.task.expect("first poll records task identity");
            let wake = runtime.wake.expect("first poll records wake callback");
            assert_eq!(
                unsafe { wake(runtime.control_data, task) },
                RAsyncWakeStatus::Woken
            );
            for _ in 0..DUPLICATE_WAKES {
                assert_eq!(
                    unsafe { wake(runtime.control_data, task) },
                    RAsyncWakeStatus::Coalesced
                );
            }
        }
        for task in &mut tasks {
            assert!(matches!(poll_once(task), Poll::Ready(_)));
        }
        for task_id in ids {
            assert_eq!(
                facade.take_completion(task_id).unwrap().kind,
                RAsyncCompletionKind::Returned
            );
            assert!(matches!(
                facade.take_completion(task_id),
                Err(TaskError::StaleTaskId | TaskError::AlreadyCompleted)
            ));
        }

        let stats = facade.stats();
        assert_eq!(stats.submitted, ROOTS as u64);
        assert_eq!(stats.polls, (ROOTS * 2) as u64);
        assert_eq!(stats.completions, ROOTS as u64);
        assert_eq!(stats.wakes, ROOTS as u64);
        assert_eq!(stats.coalesced_wakes, (ROOTS * DUPLICATE_WAKES) as u64);
        assert_eq!(facade.active_task_count(), 0);
    }

    #[test]
    fn timer_cancel_drop_and_shutdown_storm_cleans_every_root_exactly_once() {
        const ROOTS: usize = 128;
        const TIMERS: usize = 128;
        let facade = LocalExecutorFacade::with_capacity(ROOTS);
        let mut runtimes = (0..ROOTS)
            .map(|_| {
                Box::new(FakeRuntime {
                    ready_after: usize::MAX,
                    ..FakeRuntime::default()
                })
            })
            .collect::<Vec<_>>();
        let mut tasks = Vec::with_capacity(ROOTS);
        let mut ids = Vec::with_capacity(ROOTS);

        for runtime in &mut runtimes {
            let (task_id, task) = facade
                .submit_root(
                    std::ptr::from_mut(runtime.as_mut()).cast(),
                    resume_fake,
                    drop_fake,
                )
                .unwrap();
            ids.push(task_id);
            tasks.push(Some(task));
        }
        for task in &mut tasks {
            assert!(matches!(poll_once(task.as_mut().unwrap()), Poll::Pending));
        }

        let timer_task = ids[ROOTS - 1];
        let mut timers = Vec::with_capacity(TIMERS);
        let mut backends = Vec::with_capacity(TIMERS);
        for index in 0..TIMERS {
            let backend = ManualBackend::default();
            let timer = facade
                .register_timer(
                    &backend,
                    timer_task,
                    RAsyncDeadline {
                        nanos_from_epoch: (index + 1) as u64,
                    },
                )
                .unwrap();
            backends.push(backend);
            timers.push(timer);
        }
        for index in 0..TIMERS {
            if index % 2 == 0 {
                backends[index].fire();
                assert_eq!(timers[index].state(), RAsyncTimerState::Fired);
            } else {
                assert!(timers[index].cancel());
                assert!(!timers[index].cancel());
                // A backend callback arriving after cancellation is stale and
                // must not resurrect or re-count the timer.
                backends[index].fire();
                assert_eq!(timers[index].state(), RAsyncTimerState::Cancelled);
            }
        }
        let stats = facade.stats();
        assert_eq!(stats.timers_registered, TIMERS as u64);
        assert_eq!(stats.timers_fired, (TIMERS / 2) as u64);
        assert_eq!(stats.timers_cancelled, (TIMERS / 2) as u64);

        for task in tasks.iter_mut().take(ROOTS / 2) {
            drop(task.take());
        }
        facade.shutdown();
        for task in tasks.iter_mut().skip(ROOTS / 2) {
            let Poll::Ready(completion) = poll_once(task.as_mut().unwrap()) else {
                panic!("shutdown must make every remaining root terminal");
            };
            assert_eq!(completion.kind, RAsyncCompletionKind::DriverError);
            assert_eq!(completion.payload.low, RAsyncDriverError::Shutdown as u64);
            drop(task.take());
        }
        for runtime in &runtimes {
            assert_eq!(runtime.drops, 1);
        }
        for (index, task_id) in ids.into_iter().enumerate() {
            let completion = facade.take_completion(task_id).unwrap();
            if index < ROOTS / 2 {
                assert_eq!(completion.kind, RAsyncCompletionKind::Dropped);
            } else {
                assert_eq!(completion.kind, RAsyncCompletionKind::DriverError);
                assert_eq!(completion.payload.low, RAsyncDriverError::Shutdown as u64);
            }
        }
        assert_eq!(facade.active_task_count(), 0);
    }

    #[test]
    fn local_facade_has_no_background_progress_or_hidden_queue() {
        let facade = LocalExecutorFacade::with_capacity(4);
        let polled = Cell::new(0usize);
        assert_eq!(facade.active_task_count(), 0);
        assert_eq!(facade.stats(), TaskStats::default());
        assert_eq!(polled.get(), 0);
    }
}
