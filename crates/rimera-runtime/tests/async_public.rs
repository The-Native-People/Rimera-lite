use std::convert::Infallible;
use std::ffi::c_void;
use std::future::Future;
use std::ptr;
use std::task::{Context, Poll, Waker};
use std::time::Instant;

use rimera_abi::{RCallArguments, RGeneratorOperation, RGeneratorOutcome, RStatus, RValue};
use rimera_async_runtime::LocalBackend;
use rimera_runtime::{
    AsyncRuntimeDriver, RimeraContext, rimera_call, rimera_coroutine_function_new,
    rimera_kernel_initialize,
};

#[derive(Debug, Default)]
struct ImmediateBackend;

impl LocalBackend for ImmediateBackend {
    type Error = Infallible;
    type TimerHandle = ();

    const NAME: &'static str = "slice4-public-test";

    fn new() -> Result<Self, Self::Error> {
        Ok(Self)
    }

    fn drive_root<F: Future>(&self, future: F) -> F::Output {
        let mut future = Box::pin(future);
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("immediate public test root unexpectedly suspended"),
        }
    }

    fn register_timer<F>(&self, _deadline: Instant, _on_fire: F) -> Self::TimerHandle
    where
        F: FnOnce() + 'static,
    {
        panic!("Slice 4 public test does not register timers")
    }
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

#[test]
fn public_abi_coroutine_runs_as_exactly_one_backend_root() {
    let mut context = RimeraContext::default();
    assert_eq!(
        unsafe { rimera_kernel_initialize(&raw mut context) },
        RStatus::Ok
    );

    let name = b"root";
    let mut function = RValue::NONE;
    assert_eq!(
        unsafe {
            rimera_coroutine_function_new(
                &raw mut context,
                immediate_coroutine as *const () as *const c_void,
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
                &raw mut function,
            )
        },
        RStatus::Ok
    );

    let arguments = RCallArguments {
        positional: ptr::null(),
        positional_len: 0,
        keywords: ptr::null(),
        keyword_len: 0,
    };
    let mut coroutine = RValue::NONE;
    assert_eq!(
        unsafe {
            rimera_call(
                &raw mut context,
                &raw const function,
                &raw const arguments,
                &raw mut coroutine,
            )
        },
        RStatus::Ok
    );

    let driver = AsyncRuntimeDriver::with_capacity(&mut context, 1);
    let completion = driver.run_root(&ImmediateBackend, coroutine).unwrap();
    assert_eq!(completion.value(), Some(RValue::small_int(42)));
    assert_eq!(driver.stats().submitted, 1);
    assert_eq!(driver.stats().completions, 1);
    assert_eq!(driver.stats().rimera_wrapper_allocations, 0);
    assert_eq!(driver.active_task_count(), 0);
}
