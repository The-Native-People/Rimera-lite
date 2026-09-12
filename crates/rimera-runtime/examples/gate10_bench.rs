//! Repeatable ABI/runtime measurements. Source-to-native behavior is checked
//! separately by native_pipeline; these timings isolate the owned async core.
use std::alloc::{GlobalAlloc, Layout, System};
use std::ffi::c_void;
use std::future::Future;
use std::pin::Pin;
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use rimera_abi::*;
use rimera_async_runtime::{CompioBackend, LocalBackend, LocalExecutorFacade};
use rimera_runtime::*;

struct CountingAllocator;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let value = unsafe { System.alloc(layout) };
        if !value.is_null() {
            LIVE.fetch_add(layout.size(), Ordering::Relaxed);
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        value
    }
    unsafe fn dealloc(&self, value: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(value, layout) };
    }
    unsafe fn realloc(&self, value: *mut u8, old: Layout, size: usize) -> *mut u8 {
        let result = unsafe { System.realloc(value, old, size) };
        if !result.is_null() {
            LIVE.fetch_add(size, Ordering::Relaxed);
            LIVE.fetch_sub(old.size(), Ordering::Relaxed);
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        result
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

unsafe extern "C" fn immediate(
    _: *mut c_void,
    _: *const RValue,
    _: RGeneratorOperation,
    _: *const RValue,
    output: *mut RValue,
    outcome: *mut RGeneratorOutcome,
) -> RStatus {
    unsafe {
        output.write(RValue::small_int(1));
        outcome.write(RGeneratorOutcome::Returned);
    }
    RStatus::Ok
}

unsafe extern "C" fn suspended(
    _: *mut c_void,
    _: *const RValue,
    operation: RGeneratorOperation,
    _: *const RValue,
    output: *mut RValue,
    outcome: *mut RGeneratorOutcome,
) -> RStatus {
    unsafe {
        output.write(RValue::NONE);
        outcome.write(
            if matches!(
                operation,
                RGeneratorOperation::Throw | RGeneratorOperation::Close
            ) {
                RGeneratorOutcome::Returned
            } else {
                RGeneratorOutcome::Suspended
            },
        );
    }
    RStatus::Ok
}

fn function(context: &mut RimeraContext, code: RNativeGeneratorResume) -> RValue {
    let mut result = RValue::NONE;
    let name = b"bench";
    assert_eq!(
        unsafe {
            rimera_coroutine_function_new(
                context,
                code as *const () as *const c_void,
                name.as_ptr(),
                name.len(),
                name.as_ptr(),
                name.len(),
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                &mut result,
            )
        },
        RStatus::Ok
    );
    result
}

fn create(context: &mut RimeraContext, function: &RValue) -> RValue {
    let mut value = RValue::NONE;
    assert_eq!(
        unsafe { rimera_call_positional_rooted(context, function, ptr::null(), 0, &mut value) },
        RStatus::Ok
    );
    value
}

fn resume(context: &mut RimeraContext, value: &RValue, operation: RGeneratorOperation) {
    let mut result = RValue::NONE;
    let mut outcome = RGeneratorOutcome::Returned;
    assert_eq!(
        unsafe {
            rimera_generator_resume(
                context,
                value,
                operation,
                &RValue::NONE,
                &mut result,
                &mut outcome,
            )
        },
        RStatus::Ok
    );
}

struct WakeRoot {
    polls: usize,
    wake: bool,
}
unsafe extern "C" fn wake_root(
    state: *mut c_void,
    poll: *const RAsyncPollContext,
    output: *mut RAsyncCompletion,
) -> RAsyncPollState {
    let state = unsafe { &mut *state.cast::<WakeRoot>() };
    state.polls += 1;
    if state.polls == 1 {
        if state.wake {
            let poll = unsafe { &*poll };
            unsafe { (poll.wake)(poll.control_data, poll.task) };
        }
        RAsyncPollState::Pending
    } else {
        unsafe {
            output.write(RAsyncCompletion::new(
                RAsyncCompletionKind::Returned,
                RAsyncOpaqueValue::default(),
            ))
        };
        RAsyncPollState::Ready
    }
}
unsafe extern "C" fn drop_root(_: *mut c_void, _: RAsyncTaskId) {}

fn main() {
    let mut context = RimeraContext::default();
    assert_eq!(
        unsafe { rimera_kernel_initialize(&mut context) },
        RStatus::Ok
    );
    let mut roots = [
        function(&mut context, immediate),
        function(&mut context, suspended),
    ];
    let mut frame = RRootFrame {
        previous: ptr::null_mut(),
        slots: roots.as_mut_ptr(),
        len: roots.len(),
    };
    assert_eq!(
        unsafe { rimera_roots_push(&mut context, &mut frame) },
        RStatus::Ok
    );
    let backend = CompioBackend::new().unwrap();
    const OPS: usize = 20_000;
    // Two warmups, seven independently reported batches. Creation includes close,
    // matching the reference's creation/lifecycle workload and retaining GC cost.
    for batch in 0..9 {
        let start = Instant::now();
        for _ in 0..OPS {
            let value = create(&mut context, &roots[0]);
            resume(&mut context, &value, RGeneratorOperation::Close);
        }
        let creation = start.elapsed().as_nanos() as f64 / OPS as f64;
        let start = Instant::now();
        for _ in 0..OPS {
            let value = create(&mut context, &roots[0]);
            resume(&mut context, &value, RGeneratorOperation::Send);
        }
        let direct = start.elapsed().as_nanos() as f64 / OPS as f64;
        if batch >= 2 {
            println!("coroutine_creation_ns_per_op {creation}\ndirect_resume_ns_per_op {direct}");
        }
    }
    for batch in 0..9 {
        let facade = LocalExecutorFacade::with_capacity(1);
        let start = Instant::now();
        for _ in 0..300 {
            let mut state = WakeRoot {
                polls: 0,
                wake: true,
            };
            let (id, task) = facade
                .submit_root(ptr::from_mut(&mut state).cast(), wake_root, drop_root)
                .unwrap();
            assert_eq!(
                backend.drive_root(task).kind,
                RAsyncCompletionKind::Returned
            );
            facade.take_completion(id).unwrap();
            assert_eq!(state.polls, 2);
        }
        let ns = start.elapsed().as_nanos() as f64 / 300.0;
        if batch >= 2 {
            println!("ready_task_handoff_ns_per_op {ns}");
        }
        for _ in 0..16 {
            let mut state = WakeRoot {
                polls: 0,
                wake: false,
            };
            let (id, task) = facade
                .submit_root(ptr::from_mut(&mut state).cast(), wake_root, drop_root)
                .unwrap();
            let start = Instant::now();
            let timer = facade
                .register_timer(
                    &backend,
                    id,
                    facade.deadline_after(Duration::from_millis(1)),
                )
                .unwrap();
            backend.drive_root(task);
            let overshoot = start.elapsed().as_nanos().saturating_sub(1_000_000);
            assert_eq!(state.polls, 2, "idle timer was polled periodically");
            drop(timer);
            facade.take_completion(id).unwrap();
            if batch >= 2 {
                println!("timer_overshoot_ns {overshoot}");
            }
        }
        for _ in 0..300 {
            let value = create(&mut context, &roots[1]);
            let driver = AsyncRuntimeDriver::with_capacity(&mut context, 1);
            let mut task = driver.submit_root(value).unwrap();
            let mut poll = Context::from_waker(Waker::noop());
            assert!(matches!(Pin::new(&mut task).poll(&mut poll), Poll::Pending));
            let start = Instant::now();
            driver.request_cancel(task.id()).unwrap();
            let completion = backend.drive_root(task);
            let ns = start.elapsed().as_nanos();
            assert_eq!(completion.kind(), RAsyncCompletionKind::Returned);
            assert_eq!(driver.stats().cancellation_acknowledgements, 1);
            if batch >= 2 {
                println!("cancellation_delivery_ns {ns}");
            }
        }
    }
    // A fresh heap is essential: previously retained arena capacity must not
    // disappear from the idle-task memory measurement merely due to warmup.
    assert_eq!(
        unsafe { rimera_roots_pop(&mut context, &mut frame) },
        RStatus::Ok
    );
    drop(context);
    let mut context = RimeraContext::default();
    assert_eq!(
        unsafe { rimera_kernel_initialize(&mut context) },
        RStatus::Ok
    );
    let mut roots = [function(&mut context, suspended)];
    let mut frame = RRootFrame {
        previous: ptr::null_mut(),
        slots: roots.as_mut_ptr(),
        len: roots.len(),
    };
    assert_eq!(
        unsafe { rimera_roots_push(&mut context, &mut frame) },
        RStatus::Ok
    );
    context.collect();
    let before = LIVE.load(Ordering::Relaxed);
    let allocations = ALLOCS.load(Ordering::Relaxed);
    let mut idle = Vec::with_capacity(1_000);
    let mut idle_frame = RRootFrame {
        previous: ptr::null_mut(),
        slots: idle.as_mut_ptr(),
        len: 0,
    };
    assert_eq!(
        unsafe { rimera_roots_push(&mut context, &mut idle_frame) },
        RStatus::Ok
    );
    for _ in 0..1_000 {
        idle.push(create(&mut context, &roots[0]));
        idle_frame.len = idle.len();
    }
    let driver = AsyncRuntimeDriver::with_capacity(&mut context, 1_000);
    let mut tasks = idle
        .iter()
        .map(|value| driver.submit_root(*value).unwrap())
        .collect::<Vec<_>>();
    for task in &mut tasks {
        assert!(matches!(
            Pin::new(task).poll(&mut Context::from_waker(Waker::noop())),
            Poll::Pending
        ));
    }
    let retained = LIVE.load(Ordering::Relaxed).saturating_sub(before);
    let count = ALLOCS.load(Ordering::Relaxed) - allocations;
    println!(
        "idle_task_total_bytes {}\nidle_task_allocations {}",
        retained as f64 / 1_000.0,
        count as f64 / 1_000.0
    );
    drop(tasks);
    drop(driver);
    assert_eq!(
        unsafe { rimera_roots_pop(&mut context, &mut idle_frame) },
        RStatus::Ok
    );
    assert_eq!(
        unsafe { rimera_roots_pop(&mut context, &mut frame) },
        RStatus::Ok
    );
}
