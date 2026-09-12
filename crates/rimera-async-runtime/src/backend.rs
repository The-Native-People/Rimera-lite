use std::future::Future;
use std::io;
use std::time::Instant;

/// Compile-time contract implemented by one local Rust async runtime backend.
///
/// The trait deliberately has generic methods and is therefore not used as a
/// trait object. One executable monomorphizes one selected backend, keeping
/// backend lookup and virtual dispatch out of the poll path. The concrete
/// Compio adapter is owned by Gate 10 Slice 10; Slices 4 and 5 keep only this
/// backend-neutral contract so they cannot accidentally link or select a
/// backend ahead of that boundary.
pub trait LocalBackend {
    type Error: std::error::Error + Send + Sync + 'static;
    type TimerHandle;

    const NAME: &'static str;

    fn new() -> Result<Self, Self::Error>
    where
        Self: Sized;

    /// Drive exactly one submitted root future to completion on the backend.
    /// Direct Python child awaits remain inside that root future.
    fn drive_root<F: Future>(&self, future: F) -> F::Output;

    /// Register one wake-driven monotonic timer. Dropping the returned handle
    /// must cancel the pending backend timer when it has not fired yet.
    fn register_timer<F>(&self, deadline: Instant, on_fire: F) -> Self::TimerHandle
    where
        F: FnOnce() + 'static;
}

/// Compio is Gate 10's first concrete local backend. It owns the actual
/// scheduler, wakers, timer driver, and polling loop; Rimera does not mirror
/// those structures or add a second ready queue.
pub struct CompioBackend {
    runtime: compio::runtime::Runtime,
}

impl std::fmt::Debug for CompioBackend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CompioBackend")
            .field("name", &Self::NAME)
            .finish_non_exhaustive()
    }
}

impl LocalBackend for CompioBackend {
    type Error = io::Error;
    // Compio 0.17's JoinHandle is backed by async-task::Task. Its Drop
    // implementation marks the task cancelled before detaching it, which gives
    // this timer handle the facade's required cancel-on-drop semantics.
    type TimerHandle = compio::runtime::JoinHandle<()>;

    const NAME: &'static str = "compio";

    fn new() -> Result<Self, Self::Error> {
        Ok(Self {
            runtime: compio::runtime::Runtime::new()?,
        })
    }

    fn drive_root<F: Future>(&self, future: F) -> F::Output {
        // block_on drives the borrowed root directly, so Rimera creates no
        // backend task wrapper for direct nested Python awaits.
        self.runtime.block_on(future)
    }

    fn register_timer<F>(&self, deadline: Instant, on_fire: F) -> Self::TimerHandle
    where
        F: FnOnce() + 'static,
    {
        self.runtime.spawn(async move {
            compio::time::sleep_until(deadline).await;
            on_fire();
        })
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::ffi::c_void;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use rimera_abi::{
        RAsyncCompletion, RAsyncCompletionKind, RAsyncOpaqueValue, RAsyncPollContext,
        RAsyncPollState, RAsyncTaskId, RAsyncWakeStatus,
    };

    use super::{CompioBackend, LocalBackend};
    use crate::LocalExecutorFacade;

    #[test]
    fn compio_drives_a_non_send_local_root_directly() {
        let backend = CompioBackend::new().unwrap();
        let local = Rc::new(Cell::new(0_u32));
        let captured = Rc::clone(&local);
        let value = backend.drive_root(async move {
            captured.set(41);
            captured.get() + 1
        });
        assert_eq!(value, 42);
        assert_eq!(local.get(), 41);
    }

    #[derive(Default)]
    struct FacadeRuntime {
        polls: usize,
    }

    unsafe extern "C" fn resume_facade_root(
        runtime: *mut c_void,
        poll: *const RAsyncPollContext,
        completion: *mut RAsyncCompletion,
    ) -> RAsyncPollState {
        // SAFETY: the test submits a live FacadeRuntime and Compio drives the
        // facade-owned RootTask with valid poll/completion storage.
        let runtime = unsafe { &mut *runtime.cast::<FacadeRuntime>() };
        let poll = unsafe { *poll };
        runtime.polls += 1;
        if runtime.polls == 1 {
            // Wake through the exact backend waker Compio supplied to RootTask.
            // This proves the concrete adapter drives the facade directly rather
            // than requiring a Rimera ready queue or task-per-await wrapper.
            assert_eq!(
                unsafe { (poll.wake)(poll.control_data, poll.task) },
                RAsyncWakeStatus::Woken
            );
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

    unsafe extern "C" fn drop_facade_root(_runtime: *mut c_void, _task: RAsyncTaskId) {}

    #[test]
    fn compio_drives_the_facade_root_directly_without_rimera_wrapper_allocation() {
        let backend = CompioBackend::new().unwrap();
        let facade = LocalExecutorFacade::with_capacity(1);
        let mut runtime = FacadeRuntime::default();
        let (task_id, task) = facade
            .submit_root(
                std::ptr::from_mut(&mut runtime).cast(),
                resume_facade_root,
                drop_facade_root,
            )
            .unwrap();

        let completion = backend.drive_root(task);
        assert_eq!(completion.kind, RAsyncCompletionKind::Returned);
        assert_eq!(runtime.polls, 2);
        let stats = facade.stats();
        assert_eq!(stats.submitted, 1);
        assert_eq!(stats.polls, 2);
        assert_eq!(stats.completions, 1);
        assert_eq!(stats.wakes, 1);
        assert_eq!(stats.rimera_wrapper_allocations, 0);
        assert_eq!(
            facade.take_completion(task_id).unwrap().kind,
            RAsyncCompletionKind::Returned
        );
    }

    #[test]
    fn dropping_compio_timer_handle_cancels_callback() {
        let backend = CompioBackend::new().unwrap();
        let fired = Rc::new(Cell::new(false));
        let observed = Rc::clone(&fired);
        let timer = backend.register_timer(Instant::now() + Duration::from_millis(50), move || {
            observed.set(true)
        });
        drop(timer);
        backend.runtime.block_on(async {
            compio::time::sleep(Duration::from_millis(60)).await;
        });
        assert!(!fired.get());
    }

    #[test]
    fn compio_makes_progress_for_many_local_roots_and_a_timer() {
        let backend = CompioBackend::new().unwrap();
        let facade = LocalExecutorFacade::with_capacity(64);
        let mut runtimes = (0..64)
            .map(|_| Box::new(FacadeRuntime::default()))
            .collect::<Vec<_>>();
        let fired = Rc::new(Cell::new(false));
        let observed = Rc::clone(&fired);
        let timer = backend.register_timer(Instant::now() + Duration::from_millis(1), move || {
            observed.set(true);
        });
        let mut handles = Vec::new();
        let mut ids = Vec::new();
        for runtime in &mut runtimes {
            let (id, task) = facade
                .submit_root(
                    std::ptr::from_mut(runtime.as_mut()).cast(),
                    resume_facade_root,
                    drop_facade_root,
                )
                .unwrap();
            ids.push(id);
            // Compio owns these runnable tasks; the facade has no ready queue.
            handles.push(backend.runtime.spawn(task));
        }
        backend.drive_root(async {
            for handle in handles {
                assert_eq!(handle.await.unwrap().kind, RAsyncCompletionKind::Returned);
            }
            timer.await.unwrap();
        });
        assert!(fired.get());
        for id in ids {
            assert_eq!(
                facade.take_completion(id).unwrap().kind,
                RAsyncCompletionKind::Returned
            );
        }
        assert!(runtimes.iter().all(|runtime| runtime.polls == 2));
        assert_eq!(facade.active_task_count(), 0);
        assert_eq!(facade.stats().completions, 64);
    }
}
