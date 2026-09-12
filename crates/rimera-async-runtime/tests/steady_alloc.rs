use std::alloc::{GlobalAlloc, Layout, System};
use std::ffi::c_void;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use rimera_abi::{
    RAsyncCompletion, RAsyncCompletionKind, RAsyncOpaqueValue, RAsyncPollContext, RAsyncPollState,
    RAsyncTaskId,
};
use rimera_async_runtime::{CompioBackend, LocalBackend, LocalExecutorFacade};

struct CountingAllocator;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[derive(Default)]
struct Runtime {
    polls: usize,
    ready_after: usize,
}

unsafe extern "C" fn resume(
    runtime: *mut c_void,
    _poll: *const RAsyncPollContext,
    completion: *mut RAsyncCompletion,
) -> RAsyncPollState {
    let runtime = unsafe { &mut *runtime.cast::<Runtime>() };
    runtime.polls += 1;
    if runtime.polls < runtime.ready_after {
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

unsafe extern "C" fn drop_runtime(_runtime: *mut c_void, _task: RAsyncTaskId) {}

unsafe fn raw_waker_clone(_: *const ()) -> RawWaker {
    raw_waker()
}
unsafe fn raw_waker_noop(_: *const ()) {}

static RAW_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
    raw_waker_clone,
    raw_waker_noop,
    raw_waker_noop,
    raw_waker_noop,
);

fn raw_waker() -> RawWaker {
    RawWaker::new(std::ptr::null(), &RAW_WAKER_VTABLE)
}

fn poll_once(
    task: &mut rimera_async_runtime::RootTask,
    context: &mut Context<'_>,
) -> Poll<RAsyncCompletion> {
    Pin::new(task).poll(context)
}

#[test]
fn steady_state_root_polling_performs_zero_rimera_adapter_allocations() {
    let facade = LocalExecutorFacade::with_capacity(1);
    let mut runtime = Runtime {
        ready_after: 1_002,
        ..Runtime::default()
    };
    let (_task_id, mut task) = facade
        .submit_root(
            std::ptr::from_mut(&mut runtime).cast(),
            resume,
            drop_runtime,
        )
        .unwrap();

    let waker = unsafe { Waker::from_raw(raw_waker()) };
    let mut context = Context::from_waker(&waker);

    // Warm the stored waker once. The measured window begins only after every
    // Rimera-owned task/facade structure and the backend-facing waker exist.
    assert!(matches!(poll_once(&mut task, &mut context), Poll::Pending));
    let before = ALLOCATIONS.load(Ordering::Relaxed);

    for _ in 0..1_000 {
        assert!(matches!(poll_once(&mut task, &mut context), Poll::Pending));
    }

    let after = ALLOCATIONS.load(Ordering::Relaxed);
    assert_eq!(
        after,
        before,
        "steady-state Rimera adapter polling allocated {} times",
        after.saturating_sub(before)
    );
    assert_eq!(facade.stats().rimera_wrapper_allocations, 0);
    assert!(matches!(poll_once(&mut task, &mut context), Poll::Ready(_)));

    // Measure the real Compio scheduler/waker path as well as isolated polling.
    // Keep this in the same test so the process-wide allocator counter cannot
    // be perturbed by a concurrently running test.
    let backend = CompioBackend::new().unwrap();
    let facade = LocalExecutorFacade::with_capacity(1);
    let mut runtime = CompioMeasurement::default();
    let (_, task) = facade
        .submit_root(
            std::ptr::from_mut(&mut runtime).cast(),
            resume_on_compio,
            drop_runtime,
        )
        .unwrap();
    let completion = backend.drive_root(task);
    assert_eq!(completion.kind, RAsyncCompletionKind::Returned);
    assert_eq!(runtime.polls, 1_002);
    assert_eq!(
        runtime.after, runtime.before,
        "Compio steady polling allocated"
    );
    assert_eq!(facade.stats().polls, 1_002);
}

#[derive(Default)]
struct CompioMeasurement {
    polls: usize,
    before: usize,
    after: usize,
}

unsafe extern "C" fn resume_on_compio(
    runtime: *mut c_void,
    poll: *const RAsyncPollContext,
    completion: *mut RAsyncCompletion,
) -> RAsyncPollState {
    // SAFETY: drive_root borrows the live measurement until completion and the
    // facade provides its ABI poll and completion records for each callback.
    let runtime = unsafe { &mut *runtime.cast::<CompioMeasurement>() };
    let poll = unsafe { &*poll };
    runtime.polls += 1;
    if runtime.polls == 2 {
        runtime.before = ALLOCATIONS.load(Ordering::Relaxed);
    }
    if runtime.polls == 1_002 {
        runtime.after = ALLOCATIONS.load(Ordering::Relaxed);
        unsafe {
            completion.write(RAsyncCompletion::new(
                RAsyncCompletionKind::Returned,
                RAsyncOpaqueValue::default(),
            ));
        }
        return RAsyncPollState::Ready;
    }
    unsafe { (poll.wake)(poll.control_data, poll.task) };
    RAsyncPollState::Pending
}
