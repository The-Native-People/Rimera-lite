use rimera_abi::{RAsyncCompletionKind, RStatus, RValue};
use rimera_async_runtime::{CompioBackend, LocalBackend};

use crate::{AsyncRuntimeDriver, RimeraContext};

fn run_compio_root(context: &mut RimeraContext, coroutine: RValue) -> Result<RValue, String> {
    let backend = CompioBackend::new()
        .map_err(|error| format!("failed to initialize async backend `compio`: {error}"))?;
    let driver = AsyncRuntimeDriver::new(context);
    let completion = driver
        .run_root(&backend, coroutine)
        .map_err(|error| error.to_string())?;
    let kind = completion.kind();
    let value = completion.value();
    drop(completion);
    drop(driver);

    match kind {
        RAsyncCompletionKind::Returned => Ok(value.unwrap_or(RValue::NONE)),
        RAsyncCompletionKind::Raised => {
            let exception = value
                .ok_or_else(|| "async raised completion is missing its exception".to_owned())?;
            context.raise_value(exception, None, false)?;
            Err("async root raised an exception".to_owned())
        }
        RAsyncCompletionKind::Cancelled => {
            context.raise_error("RuntimeError", "async root was cancelled")
        }
        RAsyncCompletionKind::DriverError => {
            context.raise_error("RuntimeError", "async backend driver failed")
        }
        RAsyncCompletionKind::Dropped => {
            context.raise_error("RuntimeError", "async root was dropped before completion")
        }
    }
}

/// Installs the concrete Compio root runner into a freshly-created runtime
/// context. Only generated executables that selected Compio reference this
/// symbol, so synchronous binaries can dead-strip the complete adapter.
///
/// # Safety
/// `context` must point to a live RimeraContext allocated by the runtime ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_async_backend_select_compio(
    context: *mut RimeraContext,
) -> RStatus {
    if context.is_null() {
        return RStatus::InvalidArgument;
    }
    // SAFETY: null was rejected and the generated entry owns this context.
    let context = unsafe { &mut *context };
    context.install_async_root_runner(run_compio_root);
    RStatus::Ok
}
