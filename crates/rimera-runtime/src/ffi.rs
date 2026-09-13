use std::ffi::c_void;
use std::io::{self, IsTerminal, Write};
#[cfg(panic = "unwind")]
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

use rimera_abi::{
    ABI_VERSION, RCallArgumentKind, RCallArguments, RCodeMetadataSpec, RFormatConversion,
    RGeneratorDelegateOutcome, RGeneratorOperation, RNameSpec, RNativeModuleInitializer,
    RParameterSpec, RRootFrame, RStatus, RTypeParameterKind, RValue,
};

use crate::heap::HeapObject;
use crate::object::{
    CellObject, CodeObject, DictionaryViewKind, FastCallMetadata, FunctionKind, FunctionObject,
    Parameter, TypeAliasObject, TypeParameterObject,
};
use crate::{ParameterKind, RimeraContext, call, operations};

type StatusResult = Result<(), RStatus>;

unsafe extern "C" {
    #[link_name = "write"]
    fn c_write(fd: i32, buffer: *const c_void, count: usize) -> isize;
}

fn protect(
    context: *mut RimeraContext,
    operation: impl FnOnce(&mut RimeraContext) -> StatusResult,
) -> RStatus {
    if context.is_null() {
        return RStatus::InvalidArgument;
    }
    // SAFETY: null was rejected and callers own a context returned by this ABI.
    let context = unsafe { &mut *context };
    #[cfg(panic = "unwind")]
    let result = match catch_unwind(AssertUnwindSafe(|| operation(context))) {
        Ok(result) => result,
        Err(_) => {
            context.heap.refresh_managed_bytes();
            context.fail("native runtime panicked");
            return RStatus::Exception;
        }
    };
    #[cfg(panic = "abort")]
    let result = operation(context);

    match result {
        Ok(()) => {
            if context.enforce_heap_limit().is_err() {
                context.raise_emergency_memory_error();
                RStatus::Exception
            } else {
                RStatus::Ok
            }
        }
        Err(status) => {
            context.heap.refresh_managed_bytes();
            status
        }
    }
}

/// Protects runtime bookkeeping that cannot allocate or increase managed heap
/// size on its success path. Root-stack and reflection metadata updates must
/// not trigger a whole-heap accounting pass on every native function call.
fn protect_bookkeeping(
    context: *mut RimeraContext,
    operation: impl FnOnce(&mut RimeraContext) -> StatusResult,
) -> RStatus {
    if context.is_null() {
        return RStatus::InvalidArgument;
    }
    // SAFETY: null was rejected and callers own a context returned by this ABI.
    let context = unsafe { &mut *context };
    #[cfg(panic = "unwind")]
    let result = match catch_unwind(AssertUnwindSafe(|| operation(context))) {
        Ok(result) => result,
        Err(_) => {
            context.heap.refresh_managed_bytes();
            context.fail("native runtime panicked");
            return RStatus::Exception;
        }
    };
    #[cfg(panic = "abort")]
    let result = operation(context);

    match result {
        Ok(()) => RStatus::Ok,
        Err(status) => {
            context.heap.refresh_managed_bytes();
            status
        }
    }
}

fn with_output(
    context: *mut RimeraContext,
    output: *mut RValue,
    operation: impl FnOnce(&mut RimeraContext) -> Result<RValue, String>,
) -> RStatus {
    if output.is_null() {
        return RStatus::InvalidArgument;
    }
    protect(context, |context| match operation(context) {
        Ok(value) => {
            // SAFETY: null was rejected and the ABI requires writable output.
            unsafe { output.write(value) };
            Ok(())
        }
        Err(message) => {
            if context.raised.is_some() {
                return Err(RStatus::Exception);
            }
            let exception_type = if message == "managed heap limit exceeded" {
                "MemoryError"
            } else if message.contains("division") || message.contains("modulo by zero") {
                "ZeroDivisionError"
            } else if message.contains("index out of range") {
                "IndexError"
            } else if message.contains("has no attribute") {
                "AttributeError"
            } else if message.contains("requires")
                || message.contains("support")
                || message.contains("bases must be types")
                || message.contains("acceptable base type")
                || message.contains("duplicate base class")
                || message.contains("method resolution")
                || message.contains("metaclass conflict")
                || message.starts_with("super(")
                || message.contains("has no length")
                || message.contains("not subscriptable")
                || message.contains("index cannot fit")
                || message.starts_with("type.__new__()")
                || message == "attribute name must be string"
                || message.contains("takes no arguments")
            {
                "TypeError"
            } else {
                "RuntimeError"
            };
            record_exception(context, exception_type, message);
            Err(RStatus::Exception)
        }
    })
}

fn with_pattern_output(
    context: *mut RimeraContext,
    output: *mut RValue,
    matched: *mut u8,
    operation: impl FnOnce(&mut RimeraContext) -> Result<Option<RValue>, String>,
) -> RStatus {
    if output.is_null() || matched.is_null() {
        return RStatus::InvalidArgument;
    }
    protect(context, |context| match operation(context) {
        Ok(Some(value)) => {
            unsafe {
                output.write(value);
                matched.write(1);
            }
            Ok(())
        }
        Ok(None) => {
            unsafe {
                output.write(RValue::NONE);
                matched.write(0);
            }
            Ok(())
        }
        Err(message) => {
            if context.raised.is_none() {
                let exception_type = if message == "managed heap limit exceeded" {
                    "MemoryError"
                } else {
                    "TypeError"
                };
                record_exception(context, exception_type, message);
            }
            Err(RStatus::Exception)
        }
    })
}

fn record_exception(context: &mut RimeraContext, exception_type: &str, message: String) {
    if context
        .raise_builtin(exception_type, message.clone())
        .is_err()
    {
        if exception_type == "MemoryError" {
            context.raise_emergency_memory_error();
        } else {
            context.fail(message);
        }
    }
}

unsafe fn utf8<'a>(bytes: *const u8, len: usize) -> Result<&'a str, RStatus> {
    if bytes.is_null() && len != 0 {
        return Err(RStatus::InvalidArgument);
    }
    let bytes = if len == 0 {
        &[]
    } else {
        // SAFETY: the caller promises `len` readable bytes.
        unsafe { std::slice::from_raw_parts(bytes, len) }
    };
    std::str::from_utf8(bytes).map_err(|_| RStatus::InvalidArgument)
}

unsafe fn owned_name_specs(specs: *const RNameSpec, len: usize) -> Result<Vec<String>, RStatus> {
    if specs.is_null() && len != 0 {
        return Err(RStatus::InvalidArgument);
    }
    let specs = if len == 0 {
        &[]
    } else {
        // SAFETY: the caller promises `len` readable name specifications.
        unsafe { std::slice::from_raw_parts(specs, len) }
    };
    specs
        .iter()
        .map(|spec| {
            // SAFETY: every nested name follows the same readable UTF-8 slice contract.
            unsafe { utf8(spec.name, spec.name_len) }.map(str::to_owned)
        })
        .collect()
}

/// Creates a context for the requested ABI version.
///
/// # Safety
/// `output` must point to writable storage for one context pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_context_new(
    abi_version: u32,
    output: *mut *mut RimeraContext,
) -> RStatus {
    if abi_version != ABI_VERSION {
        return RStatus::AbiMismatch;
    }
    if output.is_null() {
        return RStatus::InvalidArgument;
    }
    let context = Box::into_raw(Box::new(RimeraContext::default()));
    // SAFETY: null was rejected and the ABI requires writable output.
    unsafe { output.write(context) };
    RStatus::Ok
}

/// Destroys a context created by [`rimera_context_new`].
///
/// # Safety
/// `context` must be null or a live context not previously freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_context_free(context: *mut RimeraContext) {
    if !context.is_null() {
        // SAFETY: contexts are created by Box::into_raw and freed exactly once.
        let mut context = unsafe { Box::from_raw(context) };
        context.finalize_for_shutdown();
        drop(context);
    }
}

/// Sets the managed-memory limit for one context in bytes.
///
/// Zero removes the limit. The budget covers managed heap estimates rather
/// than total process resident memory.
///
/// # Safety
/// `context` must be a live context returned by [`rimera_context_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_context_set_heap_limit(
    context: *mut RimeraContext,
    bytes: usize,
) -> RStatus {
    protect(context, |context| {
        context
            .set_heap_limit((bytes != 0).then_some(bytes))
            .map_err(|message| {
                record_exception(context, "MemoryError", message);
                RStatus::Exception
            })
    })
}

/// Enables the Gate 11 dynamic-compilation capability for one context.
///
/// This function installs policy only. Parser/compiler service registration and
/// native unit loading remain separate so capability-free artifacts have no
/// dynamic compiler dependency.
///
/// # Safety
/// `context` must be a live context returned by [`rimera_context_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_dynamic_compilation_enable(context: *mut RimeraContext) -> RStatus {
    protect(context, |context| {
        context.enable_dynamic_compilation();
        Ok(())
    })
}

/// Initializes the object, built-in type, globals, and exception kernel.
///
/// # Safety
/// `context` must be a live context returned by [`rimera_context_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_kernel_initialize(context: *mut RimeraContext) -> RStatus {
    protect(context, |context| {
        context.initialize_kernel().map_err(|message| {
            context.fail(message);
            RStatus::Exception
        })
    })
}

/// Returns the rooted runtime type for an `RValue` without exposing object pointers.
///
/// # Safety
/// `value` must be readable and `output` must be writable for one `RValue`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_type_of(
    context: *mut RimeraContext,
    value: *const RValue,
    output: *mut RValue,
) -> RStatus {
    if value.is_null() {
        return RStatus::InvalidArgument;
    }
    let value = unsafe { *value };
    with_output(context, output, |context| context.type_of(value))
}

/// Creates a managed first-class native function.
///
/// # Safety
/// All slices must remain readable for the call and `code` must implement
/// `RNativeFunction`. `output` must be writable.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn rimera_function_new(
    context: *mut RimeraContext,
    code: *const c_void,
    name: *const u8,
    name_len: usize,
    qualified_name: *const u8,
    qualified_name_len: usize,
    parameters: *const RParameterSpec,
    parameter_len: usize,
    closure: *const RValue,
    closure_len: usize,
    metadata: *const RCodeMetadataSpec,
    output: *mut RValue,
) -> RStatus {
    if code.is_null()
        || (parameters.is_null() && parameter_len != 0)
        || (closure.is_null() && closure_len != 0)
    {
        return RStatus::InvalidArgument;
    }
    // SAFETY: the caller supplies readable name and specification storage.
    let (Ok(name), Ok(qualified_name)) = (unsafe {
        (
            utf8(name, name_len),
            utf8(qualified_name, qualified_name_len),
        )
    }) else {
        return RStatus::InvalidArgument;
    };
    let parameter_specs = if parameter_len == 0 {
        &[]
    } else {
        // SAFETY: null was rejected and the caller supplies `parameter_len` entries.
        unsafe { std::slice::from_raw_parts(parameters, parameter_len) }
    };
    let closure = if closure_len == 0 {
        &[]
    } else {
        // SAFETY: null was rejected and the caller supplies `closure_len` values.
        unsafe { std::slice::from_raw_parts(closure, closure_len) }
    };
    let (filename, first_line, local_names, cell_names, free_names) = if metadata.is_null() {
        (String::new(), 0, Vec::new(), Vec::new(), Vec::new())
    } else {
        // SAFETY: a non-null metadata pointer is readable for one ABI record.
        let metadata = unsafe { *metadata };
        let Ok(filename) = (unsafe { utf8(metadata.filename, metadata.filename_len) }) else {
            return RStatus::InvalidArgument;
        };
        // SAFETY: nested arrays use the same readable-slice contract as parameters.
        let (Ok(local_names), Ok(cell_names), Ok(free_names)) = (unsafe {
            (
                owned_name_specs(metadata.local_names, metadata.local_name_len),
                owned_name_specs(metadata.cell_names, metadata.cell_name_len),
                owned_name_specs(metadata.free_names, metadata.free_name_len),
            )
        }) else {
            return RStatus::InvalidArgument;
        };
        (
            filename.to_owned(),
            metadata.first_line,
            local_names,
            cell_names,
            free_names,
        )
    };
    let mut converted = Vec::with_capacity(parameter_len);
    let mut positional_defaults = Vec::new();
    let mut keyword_defaults = Vec::new();
    for parameter in parameter_specs {
        // SAFETY: each parameter name follows the same readable-slice contract.
        let Ok(parameter_name) = (unsafe { utf8(parameter.name, parameter.name_len) }) else {
            return RStatus::InvalidArgument;
        };
        let Ok(kind) = ParameterKind::try_from(parameter.kind) else {
            return RStatus::InvalidArgument;
        };
        let has_default = parameter.has_default != 0;
        if has_default {
            match kind {
                ParameterKind::PositionalOnly | ParameterKind::PositionalOrKeyword => {
                    positional_defaults.push(parameter.default);
                }
                ParameterKind::KeywordOnly => {
                    keyword_defaults.push((parameter_name.to_owned(), parameter.default));
                }
                ParameterKind::VarArgs | ParameterKind::VarKeywords => {
                    return RStatus::InvalidArgument;
                }
            }
        }
        converted.push(Parameter {
            name: parameter_name.to_owned(),
            kind,
            has_default,
        });
    }
    let fast_positional_arity = converted
        .iter()
        .all(|parameter| {
            matches!(
                parameter.kind,
                ParameterKind::PositionalOnly | ParameterKind::PositionalOrKeyword
            )
        })
        .then_some(converted.len());
    let mut roots = closure.to_vec();
    roots.extend_from_slice(&positional_defaults);
    roots.extend(keyword_defaults.iter().map(|(_, value)| *value));
    with_output(context, output, |context| {
        context.initialize_kernel()?;
        context.with_temporary_roots(&roots, |context| {
            let defaults = (!positional_defaults.is_empty())
                .then(|| operations::tuple(context, &positional_defaults))
                .transpose()?;

            let mut metadata_roots = roots.clone();
            defaults
                .into_iter()
                .for_each(|value| metadata_roots.push(value));
            let keyword_defaults_value = if keyword_defaults.is_empty() {
                None
            } else {
                let mut keys = Vec::with_capacity(keyword_defaults.len());
                for (key, _) in &keyword_defaults {
                    let key = context.with_temporary_roots(&metadata_roots, |context| {
                        operations::string(context, key)
                    })?;
                    metadata_roots.push(key);
                    keys.push(key);
                }
                let values = keyword_defaults
                    .iter()
                    .map(|(_, value)| *value)
                    .collect::<Vec<_>>();
                Some(context.with_temporary_roots(&metadata_roots, |context| {
                    operations::dictionary(context, &keys, &values)
                })?)
            };
            keyword_defaults_value
                .into_iter()
                .for_each(|value| metadata_roots.push(value));
            let closure_value = if closure.is_empty() {
                None
            } else {
                let value = context.with_temporary_roots(&metadata_roots, |context| {
                    operations::tuple(context, closure)
                })?;
                metadata_roots.push(value);
                Some(value)
            };
            let code_value = context.with_temporary_roots(&metadata_roots, |context| {
                let native_unit_address = context.active_dynamic_unit_address();
                context.allocate(HeapObject::Code(CodeObject {
                    dynamic_mode: None,
                    flags_override: None,
                    native_unit_address,
                    code_address: code as usize,
                    kind: FunctionKind::Normal,
                    name: name.to_owned(),
                    qualified_name: qualified_name.to_owned(),
                    parameters: converted.into_boxed_slice(),
                    filename,
                    first_line,
                    local_names: local_names.into_boxed_slice(),
                    cell_names: cell_names.into_boxed_slice(),
                    free_names: free_names.into_boxed_slice(),
                }))
            })?;
            metadata_roots.push(code_value);
            let globals = context
                .globals()
                .ok_or_else(|| "function has no defining module namespace".to_owned())?;
            metadata_roots.push(globals);
            context.with_temporary_roots(&metadata_roots, |context| {
                let function = context.allocate(HeapObject::Function(FunctionObject {
                    code: code_value,
                    fast_call: FastCallMetadata::new(
                        code as usize,
                        first_line,
                        fast_positional_arity,
                        FunctionKind::Normal,
                    ),
                    globals,
                    name: name.to_owned(),
                    qualified_name: qualified_name.to_owned(),
                    closure: closure_value,
                    defaults,
                    keyword_defaults: keyword_defaults_value,
                    annotations: None,
                    type_params: None,
                }))?;
                context.capture_function_builtins(function, globals);
                Ok(function)
            })
        })
    })
}

/// Creates one managed Python 3.12 type parameter.
///
/// # Safety
/// `name` must be readable UTF-8 and `output` writable for one `RValue`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_type_parameter_new(
    context: *mut RimeraContext,
    kind: u8,
    name: *const u8,
    name_len: usize,
    output: *mut RValue,
) -> RStatus {
    let Ok(kind) = RTypeParameterKind::try_from(kind) else {
        return RStatus::InvalidArgument;
    };
    let Ok(name) = (unsafe { utf8(name, name_len) }) else {
        return RStatus::InvalidArgument;
    };
    with_output(context, output, |context| {
        context.allocate(HeapObject::TypeParameter(TypeParameterObject {
            name: name.to_owned(),
            kind,
        }))
    })
}

/// Creates one managed Python 3.12 `type` statement alias object.
///
/// # Safety
/// Name storage and both input values must be readable; `output` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_type_alias_new(
    context: *mut RimeraContext,
    name: *const u8,
    name_len: usize,
    type_params: *const RValue,
    value: *const RValue,
    output: *mut RValue,
) -> RStatus {
    if type_params.is_null() || value.is_null() {
        return RStatus::InvalidArgument;
    }
    let Ok(name) = (unsafe { utf8(name, name_len) }) else {
        return RStatus::InvalidArgument;
    };
    let (type_params, value) = unsafe { (*type_params, *value) };
    with_output(context, output, |context| {
        if !matches!(context.heap.get(type_params), Some(HeapObject::Tuple(_))) {
            return Err("type alias parameters must be a tuple".to_owned());
        }
        context.with_temporary_roots(&[type_params, value], |context| {
            context.allocate(HeapObject::TypeAlias(TypeAliasObject {
                name: name.to_owned(),
                type_params,
                value,
            }))
        })
    })
}

/// Creates a compiled generator function. The normal function constructor is
/// reused for the shared binding metadata, then the managed function is marked
/// with the number of persistent generator slots emitted by codegen.
///
/// # Safety
/// This follows the same pointer contract as [`rimera_function_new`].
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn rimera_generator_function_new(
    context: *mut RimeraContext,
    code: *const c_void,
    name: *const u8,
    name_len: usize,
    qualified_name: *const u8,
    qualified_name_len: usize,
    parameters: *const RParameterSpec,
    parameter_len: usize,
    closure: *const RValue,
    closure_len: usize,
    metadata: *const RCodeMetadataSpec,
    persistent_slot_count: usize,
    output: *mut RValue,
) -> RStatus {
    let status = unsafe {
        rimera_function_new(
            context,
            code,
            name,
            name_len,
            qualified_name,
            qualified_name_len,
            parameters,
            parameter_len,
            closure,
            closure_len,
            metadata,
            output,
        )
    };
    if status != RStatus::Ok || context.is_null() || output.is_null() {
        return status;
    }
    // SAFETY: success guarantees an initialized output and the context remains
    // owned by the caller for this ABI call.
    let (context, function) = unsafe { (&mut *context, *output) };
    let code = match context.heap.get(function) {
        Some(HeapObject::Function(function)) => function.code,
        _ => return RStatus::InvalidArgument,
    };
    let kind = FunctionKind::Generator {
        persistent_slot_count,
    };
    match context.heap.get_mut(code) {
        Some(HeapObject::Code(code)) => code.kind = kind,
        _ => return RStatus::InvalidArgument,
    }
    match context.heap.get_mut(function) {
        Some(HeapObject::Function(function)) => {
            function.fast_call.set_kind(kind);
            RStatus::Ok
        }
        _ => RStatus::InvalidArgument,
    }
}

/// Creates a compiled native coroutine function. Calling the resulting
/// function remains lazy: argument binding allocates coroutine state but does
/// not execute the body until the first send/await resume.
///
/// # Safety
/// This follows the same pointer contract as [`rimera_function_new`].
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn rimera_coroutine_function_new(
    context: *mut RimeraContext,
    code: *const c_void,
    name: *const u8,
    name_len: usize,
    qualified_name: *const u8,
    qualified_name_len: usize,
    parameters: *const RParameterSpec,
    parameter_len: usize,
    closure: *const RValue,
    closure_len: usize,
    metadata: *const RCodeMetadataSpec,
    persistent_slot_count: usize,
    output: *mut RValue,
) -> RStatus {
    let status = unsafe {
        rimera_function_new(
            context,
            code,
            name,
            name_len,
            qualified_name,
            qualified_name_len,
            parameters,
            parameter_len,
            closure,
            closure_len,
            metadata,
            output,
        )
    };
    if status != RStatus::Ok || context.is_null() || output.is_null() {
        return status;
    }
    // SAFETY: success guarantees an initialized output and a live context.
    let (context, function) = unsafe { (&mut *context, *output) };
    let code = match context.heap.get(function) {
        Some(HeapObject::Function(function)) => function.code,
        _ => return RStatus::InvalidArgument,
    };
    let kind = FunctionKind::Coroutine {
        persistent_slot_count,
    };
    match context.heap.get_mut(code) {
        Some(HeapObject::Code(code)) => code.kind = kind,
        _ => return RStatus::InvalidArgument,
    }
    match context.heap.get_mut(function) {
        Some(HeapObject::Function(function)) => {
            function.fast_call.set_kind(kind);
            RStatus::Ok
        }
        _ => RStatus::InvalidArgument,
    }
}

/// Attaches the ordinary-call ABI entry emitted for a coroutine whose MIR has
/// been proven unable to suspend. This does not change ordinary Python call
/// behavior: calling the function still constructs a lazy coroutine object.
/// The entry is consulted only by the guarded `await f(...)` fusion operation.
///
/// # Safety
/// `function` must point to a live Rimera coroutine function owned by
/// `context`; `ready_code` must be a stable native function address emitted in
/// the same executable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_function_ready_coroutine_set(
    context: *mut RimeraContext,
    function: *const RValue,
    ready_code: *const c_void,
    repeat_pure: u8,
) -> RStatus {
    if context.is_null() || function.is_null() || ready_code.is_null() || repeat_pure > 1 {
        return RStatus::InvalidArgument;
    }
    let (context, function) = unsafe { (&mut *context, *function) };
    let Some(HeapObject::Function(object)) = context.heap.get_mut(function) else {
        return RStatus::InvalidArgument;
    };
    if !matches!(object.fast_call.kind(), FunctionKind::Coroutine { .. }) {
        return RStatus::InvalidArgument;
    }
    object
        .fast_call
        .set_ready_coroutine(ready_code as usize, repeat_pure != 0);
    RStatus::Ok
}

/// Creates a compiled native async-generator function. Calling it is lazy and
/// allocates only the suspended async-generator state; protocol operations are
/// represented by separate managed awaitable objects.
///
/// # Safety
/// This follows the same pointer contract as [`rimera_function_new`].
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn rimera_async_generator_function_new(
    context: *mut RimeraContext,
    code: *const c_void,
    name: *const u8,
    name_len: usize,
    qualified_name: *const u8,
    qualified_name_len: usize,
    parameters: *const RParameterSpec,
    parameter_len: usize,
    closure: *const RValue,
    closure_len: usize,
    metadata: *const RCodeMetadataSpec,
    persistent_slot_count: usize,
    output: *mut RValue,
) -> RStatus {
    let status = unsafe {
        rimera_function_new(
            context,
            code,
            name,
            name_len,
            qualified_name,
            qualified_name_len,
            parameters,
            parameter_len,
            closure,
            closure_len,
            metadata,
            output,
        )
    };
    if status != RStatus::Ok || context.is_null() || output.is_null() {
        return status;
    }
    let (context, function) = unsafe { (&mut *context, *output) };
    let code = match context.heap.get(function) {
        Some(HeapObject::Function(function)) => function.code,
        _ => return RStatus::InvalidArgument,
    };
    let kind = FunctionKind::AsyncGenerator {
        persistent_slot_count,
    };
    match context.heap.get_mut(code) {
        Some(HeapObject::Code(code)) => code.kind = kind,
        _ => return RStatus::InvalidArgument,
    }
    match context.heap.get_mut(function) {
        Some(HeapObject::Function(function)) => {
            function.fast_call.set_kind(kind);
            RStatus::Ok
        }
        _ => RStatus::InvalidArgument,
    }
}

/// Allocates the suspended state for one already-bound generator call.
///
/// # Safety
/// `function` and non-empty `bound` slices must be readable; `output` is
/// writable for one value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_generator_new(
    context: *mut RimeraContext,
    function: *const RValue,
    bound: *const RValue,
    bound_len: usize,
    output: *mut RValue,
) -> RStatus {
    if function.is_null() || (bound.is_null() && bound_len != 0) {
        return RStatus::InvalidArgument;
    }
    let function = unsafe { *function };
    let bound = if bound_len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(bound, bound_len) }
    };
    with_output(context, output, |context| {
        context.new_generator(function, bound)
    })
}

/// Resumes a generator through its compiled native state machine.
///
/// # Safety
/// Every pointer must remain readable/writable for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_generator_resume(
    context: *mut RimeraContext,
    generator: *const RValue,
    operation: rimera_abi::RGeneratorOperation,
    input: *const RValue,
    output: *mut RValue,
    outcome: *mut rimera_abi::RGeneratorOutcome,
) -> RStatus {
    if generator.is_null() || input.is_null() || output.is_null() || outcome.is_null() {
        return RStatus::InvalidArgument;
    }
    let (generator, input) = unsafe { (*generator, *input) };
    let Some(context) = (unsafe { context.as_mut() }) else {
        return RStatus::InvalidArgument;
    };
    match context.resume_generator(generator, operation, input) {
        Ok(result) => {
            unsafe {
                output.write(result.value);
                outcome.write(result.outcome);
            }
            RStatus::Ok
        }
        Err(message) => {
            if context.raised.is_none() {
                context.fail(message);
            }
            RStatus::Exception
        }
    }
}

/// Advances a freshly-created `yield from` delegate once through its generic
/// iterator protocol. `value` is either the yielded item or completion value.
///
/// # Safety
/// `iterator` must be readable and both outputs writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_generator_delegate_start(
    context: *mut RimeraContext,
    iterator: *const RValue,
    value: *mut RValue,
    outcome: *mut RGeneratorDelegateOutcome,
) -> RStatus {
    if iterator.is_null() || value.is_null() || outcome.is_null() {
        return RStatus::InvalidArgument;
    }
    let iterator = unsafe { *iterator };
    protect(context, |context| {
        match call::generator_delegate_start(context, iterator) {
            Ok((result, result_outcome)) => {
                unsafe {
                    value.write(result);
                    outcome.write(result_outcome);
                }
                Ok(())
            }
            Err(message) => {
                if context.raised.is_none() {
                    context.fail(message);
                }
                Err(RStatus::Exception)
            }
        }
    })
}

/// Publishes or clears the active `yield from` delegate on a generator object.
///
/// # Safety
/// `generator` and `delegate` must be readable ABI values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_generator_delegate_set(
    context: *mut RimeraContext,
    generator: *const RValue,
    delegate: *const RValue,
) -> RStatus {
    if generator.is_null() || delegate.is_null() {
        return RStatus::InvalidArgument;
    }
    let (generator, delegate) = unsafe { (*generator, *delegate) };
    protect(context, |context| {
        let Some(HeapObject::Generator(object)) = context.heap.get_mut(generator) else {
            return Err(RStatus::InvalidArgument);
        };
        object.delegate = (delegate != RValue::NONE).then_some(delegate);
        Ok(())
    })
}

/// Forwards one resume operation through the currently-published `yield from`
/// delegate. The helper clears the delegate after completion or propagation.
///
/// # Safety
/// Generator/input must be readable and outputs writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_generator_delegate_resume(
    context: *mut RimeraContext,
    generator: *const RValue,
    operation: RGeneratorOperation,
    input: *const RValue,
    value: *mut RValue,
    outcome: *mut RGeneratorDelegateOutcome,
) -> RStatus {
    if generator.is_null() || input.is_null() || value.is_null() || outcome.is_null() {
        return RStatus::InvalidArgument;
    }
    let (generator, input) = unsafe { (*generator, *input) };
    protect(context, |context| {
        let delegate = match context.heap.get(generator) {
            Some(HeapObject::Generator(object)) => object.delegate,
            _ => return Err(RStatus::InvalidArgument),
        };
        let Some(delegate) = delegate else {
            return Err(RStatus::InvalidArgument);
        };
        match call::generator_delegate_resume(context, delegate, operation, input) {
            Ok((result, result_outcome)) => {
                if result_outcome != RGeneratorDelegateOutcome::Yielded
                    && let Some(HeapObject::Generator(object)) = context.heap.get_mut(generator)
                {
                    object.delegate = None;
                }
                unsafe {
                    value.write(result);
                    outcome.write(result_outcome);
                }
                Ok(())
            }
            Err(message) => {
                if let Some(HeapObject::Generator(object)) = context.heap.get_mut(generator) {
                    object.delegate = None;
                }
                if context.raised.is_none() {
                    context.fail(message);
                }
                Err(RStatus::Exception)
            }
        }
    })
}

/// Reads the compiled function owned by a suspended generator.
///
/// # Safety
/// `generator` must be readable and `output` writable for one `RValue`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_generator_function_get(
    context: *mut RimeraContext,
    generator: *const RValue,
    output: *mut RValue,
) -> RStatus {
    if context.is_null() || generator.is_null() || output.is_null() {
        return RStatus::InvalidArgument;
    }
    let (context, generator) = unsafe { (&mut *context, *generator) };
    let Some(HeapObject::Generator(generator)) = context.heap.get(generator) else {
        return RStatus::InvalidArgument;
    };
    unsafe { output.write(generator.function) };
    RStatus::Ok
}

/// Reads the stable suspension state selected by generated code.
///
/// # Safety
/// `generator` must be readable and `output` writable for one `u32`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_generator_state_get(
    context: *mut RimeraContext,
    generator: *const RValue,
    output: *mut u32,
) -> RStatus {
    if context.is_null() || generator.is_null() || output.is_null() {
        return RStatus::InvalidArgument;
    }
    let (context, generator) = unsafe { (&mut *context, *generator) };
    let Some(HeapObject::Generator(generator)) = context.heap.get(generator) else {
        return RStatus::InvalidArgument;
    };
    unsafe { output.write(generator.state) };
    RStatus::Ok
}

/// Publishes a stable suspension state after all live values have been saved.
///
/// # Safety
/// `generator` must be readable for one `RValue`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_generator_state_set(
    context: *mut RimeraContext,
    generator: *const RValue,
    state: u32,
) -> RStatus {
    if context.is_null() || generator.is_null() {
        return RStatus::InvalidArgument;
    }
    let (context, generator) = unsafe { (&mut *context, *generator) };
    let Some(HeapObject::Generator(generator)) = context.heap.get_mut(generator) else {
        return RStatus::InvalidArgument;
    };
    generator.state = state;
    RStatus::Ok
}

/// Publishes the source line associated with the generator's current
/// suspension point. This updates only Python-visible frame metadata; native
/// resume state remains opaque and separately owned by `rimera_generator_state_set`.
///
/// # Safety
/// `generator` must be readable for one `RValue`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_generator_frame_line_set(
    context: *mut RimeraContext,
    generator: *const RValue,
    line: u32,
) -> RStatus {
    if context.is_null() || generator.is_null() {
        return RStatus::InvalidArgument;
    }
    let (context, generator) = unsafe { (&mut *context, *generator) };
    match context.set_generator_frame_line(generator, line) {
        Ok(()) => RStatus::Ok,
        Err(message) => {
            context.fail(message);
            RStatus::InvalidArgument
        }
    }
}

/// Restores one compiler-planned value that is live across suspension.
///
/// # Safety
/// `generator` must be readable and `output` writable for one `RValue`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_generator_slot_get(
    context: *mut RimeraContext,
    generator: *const RValue,
    index: usize,
    output: *mut RValue,
) -> RStatus {
    if context.is_null() || generator.is_null() || output.is_null() {
        return RStatus::InvalidArgument;
    }
    let (context, generator) = unsafe { (&mut *context, *generator) };
    let Some(HeapObject::Generator(generator)) = context.heap.get(generator) else {
        return RStatus::InvalidArgument;
    };
    let Some(value) = generator.slots.get(index).copied().flatten() else {
        return RStatus::InvalidArgument;
    };
    unsafe { output.write(value) };
    RStatus::Ok
}

/// Saves one compiler-planned value that is live across suspension.
///
/// # Safety
/// `generator` and `value` must each be readable for one `RValue`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_generator_slot_set(
    context: *mut RimeraContext,
    generator: *const RValue,
    index: usize,
    value: *const RValue,
) -> RStatus {
    if context.is_null() || generator.is_null() || value.is_null() {
        return RStatus::InvalidArgument;
    }
    let (context, generator, value) = unsafe { (&mut *context, *generator, *value) };
    let Some(HeapObject::Generator(generator)) = context.heap.get_mut(generator) else {
        return RStatus::InvalidArgument;
    };
    let Some(slot) = generator.slots.get_mut(index) else {
        return RStatus::InvalidArgument;
    };
    *slot = Some(value);
    RStatus::Ok
}

/// Creates a mutable closure cell. A null `initial` creates an unbound cell.
///
/// # Safety
/// Non-null pointers must be readable and `output` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_cell_new(
    context: *mut RimeraContext,
    initial: *const RValue,
    output: *mut RValue,
) -> RStatus {
    let initial = if initial.is_null() {
        None
    } else {
        // SAFETY: the non-null input points to a live value.
        Some(unsafe { *initial })
    };
    with_output(context, output, |context| {
        context.with_temporary_roots(&initial.into_iter().collect::<Vec<_>>(), |context| {
            context.allocate(HeapObject::Cell(CellObject { value: initial }))
        })
    })
}

/// Configures the Python-visible namespace semantics for the active compiled
/// activation. A null namespace selects function-style snapshot locals.
///
/// # Safety
/// A non-null namespace pointer must be readable for one `RValue`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_reflection_scope_configure(
    context: *mut RimeraContext,
    namespace: *const RValue,
    comprehension: u8,
) -> RStatus {
    if context.is_null() {
        return RStatus::InvalidArgument;
    }
    let namespace = if namespace.is_null() {
        None
    } else {
        Some(unsafe { *namespace })
    };
    protect_bookkeeping(context, |context| {
        context
            .configure_active_scope(namespace, comprehension != 0)
            .map_err(|message| {
                record_exception(context, "RuntimeError", message);
                RStatus::Exception
            })
    })
}

/// Registers the contiguous native-local array owned by the current compiled
/// stack activation. The storage is also part of the native GC root frame, so
/// this pointer is reflection metadata rather than a second ownership path.
///
/// # Safety
/// `values` must remain readable for `len` values until the active native call
/// is popped. A null pointer is valid only when `len == 0`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_reflection_native_locals_register(
    context: *mut RimeraContext,
    values: *const RValue,
    len: usize,
) -> RStatus {
    if context.is_null() || (values.is_null() && len != 0) {
        return RStatus::InvalidArgument;
    }
    protect_bookkeeping(context, |context| {
        context
            .register_active_native_locals(values, len)
            .map_err(|message| {
                record_exception(context, "RuntimeError", message);
                RStatus::Exception
            })
    })
}

/// Registers one authoritative compiled local cell with the active activation.
/// `locals()` refreshes values from these cells rather than from shadow frame
/// storage.
///
/// # Safety
/// Name storage and `cell` must be readable for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_reflection_local_register(
    context: *mut RimeraContext,
    name: *const u8,
    name_len: usize,
    cell: *const RValue,
) -> RStatus {
    if context.is_null() || cell.is_null() {
        return RStatus::InvalidArgument;
    }
    let Ok(name) = (unsafe { utf8(name, name_len) }) else {
        return RStatus::InvalidArgument;
    };
    let cell = unsafe { *cell };
    protect_bookkeeping(context, |context| {
        context
            .register_active_local(name, cell)
            .map_err(|message| {
                record_exception(context, "RuntimeError", message);
                RStatus::Exception
            })
    })
}

/// Raises the CPython-shaped error for an unbound native local. This is a cold
/// path used only after generated code has tested its stack-local sentinel.
///
/// # Safety
/// `name` must reference `name_len` readable UTF-8 bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_unbound_local(
    context: *mut RimeraContext,
    name: *const u8,
    name_len: usize,
) -> RStatus {
    if context.is_null() {
        return RStatus::InvalidArgument;
    }
    let Ok(name) = (unsafe { utf8(name, name_len) }) else {
        return RStatus::InvalidArgument;
    };
    protect(context, |context| {
        record_exception(
            context,
            "UnboundLocalError",
            format!(
                "cannot access local variable '{name}' where it is not associated with a value"
            ),
        );
        Err(RStatus::Exception)
    })
}

/// Reads a closure cell.
///
/// # Safety
/// `cell` must be readable and `output` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_cell_get(
    context: *mut RimeraContext,
    cell: *const RValue,
    output: *mut RValue,
) -> RStatus {
    if cell.is_null() || output.is_null() {
        return RStatus::InvalidArgument;
    }
    let cell = unsafe { *cell };
    protect(context, |context| {
        let value = match context.heap.get(cell) {
            Some(HeapObject::Cell(cell)) => cell.value,
            _ => return Err(RStatus::InvalidArgument),
        };
        let Some(value) = value else {
            record_exception(
                context,
                "UnboundLocalError",
                "cannot access free variable where it is not associated with a value".to_owned(),
            );
            return Err(RStatus::Exception);
        };
        unsafe { output.write(value) };
        Ok(())
    })
}

/// Reads a cell and reports CPython-shaped unbound-name diagnostics.
///
/// # Safety
/// Cell and name inputs must be readable and `output` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_cell_get_named(
    context: *mut RimeraContext,
    cell: *const RValue,
    name: *const u8,
    name_len: usize,
    free: u8,
    output: *mut RValue,
) -> RStatus {
    if cell.is_null() || output.is_null() {
        return RStatus::InvalidArgument;
    }
    let Ok(name) = (unsafe { utf8(name, name_len) }) else {
        return RStatus::InvalidArgument;
    };
    let cell = unsafe { *cell };
    protect(context, |context| {
        let value = match context.heap.get(cell) {
            Some(HeapObject::Cell(cell)) => cell.value,
            _ => return Err(RStatus::InvalidArgument),
        };
        let Some(value) = value else {
            let message = if free != 0 {
                format!(
                    "cannot access free variable '{name}' where it is not associated with a value in enclosing scope"
                )
            } else {
                format!(
                    "cannot access local variable '{name}' where it is not associated with a value"
                )
            };
            record_exception(
                context,
                if free != 0 {
                    "NameError"
                } else {
                    "UnboundLocalError"
                },
                message,
            );
            return Err(RStatus::Exception);
        };
        unsafe { output.write(value) };
        Ok(())
    })
}

/// Stores a value in a closure cell.
///
/// # Safety
/// Both inputs must be readable values owned by this context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_cell_set(
    context: *mut RimeraContext,
    cell: *const RValue,
    value: *const RValue,
) -> RStatus {
    if cell.is_null() || value.is_null() {
        return RStatus::InvalidArgument;
    }
    let (cell, value) = unsafe { (*cell, *value) };
    protect(context, |context| match context.heap.get_mut(cell) {
        Some(HeapObject::Cell(cell)) => {
            cell.value = Some(value);
            Ok(())
        }
        _ => Err(RStatus::InvalidArgument),
    })
}

/// Clears a closure/local cell to Python's unbound state.
///
/// # Safety
/// `cell` must be a readable value owned by the context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_cell_clear(
    context: *mut RimeraContext,
    cell: *const RValue,
) -> RStatus {
    if cell.is_null() {
        return RStatus::InvalidArgument;
    }
    let cell = unsafe { *cell };
    protect(context, |context| match context.heap.get_mut(cell) {
        Some(HeapObject::Cell(cell)) => {
            cell.value = None;
            Ok(())
        }
        _ => Err(RStatus::InvalidArgument),
    })
}

/// Reads one closure cell handle from a managed function.
///
/// # Safety
/// `function` must be readable and `output` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_function_closure_get(
    context: *mut RimeraContext,
    function: *const RValue,
    index: usize,
    output: *mut RValue,
) -> RStatus {
    if function.is_null() || output.is_null() {
        return RStatus::InvalidArgument;
    }
    let function = unsafe { *function };
    protect(context, |context| {
        let value = match context.heap.get(function) {
            Some(HeapObject::Function(function)) => {
                function
                    .closure
                    .and_then(|closure| match context.heap.get(closure) {
                        Some(HeapObject::Tuple(cells)) => cells.get(index).copied(),
                        _ => None,
                    })
            }
            _ => return Err(RStatus::InvalidArgument),
        };
        let Some(value) = value else {
            context.fail("closure cell index is out of range");
            return Err(RStatus::InvalidArgument);
        };
        unsafe { output.write(value) };
        Ok(())
    })
}

/// Stores a module global by UTF-8 name.
///
/// # Safety
/// Name and value storage must remain readable for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_global_set(
    context: *mut RimeraContext,
    name: *const u8,
    name_len: usize,
    value: *const RValue,
) -> RStatus {
    if value.is_null() {
        return RStatus::InvalidArgument;
    }
    let Ok(name) = (unsafe { utf8(name, name_len) }) else {
        return RStatus::InvalidArgument;
    };
    let value = unsafe { *value };
    protect(context, |context| {
        context
            .initialize_kernel()
            .map_err(|_| RStatus::Exception)?;
        let globals = context.globals().ok_or(RStatus::InvalidArgument)?;
        let globals = context
            .dictionary_storage(globals)
            .ok_or(RStatus::InvalidArgument)?;
        context
            .namespace_set(globals, name, value)
            .map_err(|message| {
                record_exception(context, "RuntimeError", message);
                RStatus::Exception
            })
    })
}

/// Imports one explicitly registered native module shell and returns its cached
/// managed module object. This is the pulled-forward import foundation; it does
/// not execute Python module source or expose `__import__`.
///
/// # Safety
/// Name storage must be readable and `output` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_import_name(
    context: *mut RimeraContext,
    name: *const u8,
    name_len: usize,
    output: *mut RValue,
) -> RStatus {
    let Ok(name) = (unsafe { utf8(name, name_len) }) else {
        return RStatus::InvalidArgument;
    };
    with_output(context, output, |context| context.import_name(name))
}

/// Executes an import statement through the current managed
/// `builtins.__import__` binding. Generated code passes the active module
/// globals and the ordinary empty fromlist/absolute-level arguments.
///
/// # Safety
/// Name storage must be readable and `output` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_import_dispatch(
    context: *mut RimeraContext,
    name: *const u8,
    name_len: usize,
    output: *mut RValue,
) -> RStatus {
    let Ok(name) = (unsafe { utf8(name, name_len) }) else {
        return RStatus::InvalidArgument;
    };
    with_output(context, output, |context| {
        context.initialize_kernel()?;
        let Some(callable) = context.lookup_builtin("__import__") else {
            return context.raise_error("ImportError", "__import__ not found");
        };
        let globals = context.globals().unwrap_or(RValue::NONE);
        context.with_temporary_roots(&[callable, globals], |context| {
            let name = operations::string(context, name)?;
            context.with_temporary_roots(&[callable, globals, name], |context| {
                call::invoke(
                    context,
                    callable,
                    &[name, globals, globals, RValue::NONE, RValue::small_int(0)],
                    &[],
                )
            })
        })
    })
}

/// Registers one statically linked source module with the context import
/// manifest. Registration does not create or cache a module object.
///
/// # Safety
/// String storage must be readable and `initializer` must point at a generated
/// function with the documented module-initializer ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_register_source_module(
    context: *mut RimeraContext,
    name: *const u8,
    name_len: usize,
    filename: *const u8,
    filename_len: usize,
    package: *const u8,
    package_len: usize,
    is_package: u8,
    initializer: *const c_void,
) -> RStatus {
    if initializer.is_null() {
        return RStatus::InvalidArgument;
    }
    let Ok(name) = (unsafe { utf8(name, name_len) }) else {
        return RStatus::InvalidArgument;
    };
    let Ok(filename) = (unsafe { utf8(filename, filename_len) }) else {
        return RStatus::InvalidArgument;
    };
    let Ok(package) = (unsafe { utf8(package, package_len) }) else {
        return RStatus::InvalidArgument;
    };
    let initializer =
        unsafe { std::mem::transmute::<*const c_void, RNativeModuleInitializer>(initializer) };
    protect(context, |context| {
        context
            .register_source_module(name, filename, package, is_package != 0, initializer)
            .map_err(|message| {
                record_exception(context, "RuntimeError", message);
                RStatus::Exception
            })
    })
}

/// Registers one namespace package from the entry object's static manifest.
/// Ordered locations are separated by NUL bytes.
///
/// # Safety
/// Name and location storage must remain readable for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_register_namespace_module(
    context: *mut RimeraContext,
    name: *const u8,
    name_len: usize,
    locations: *const u8,
    locations_len: usize,
) -> RStatus {
    let Ok(name) = (unsafe { utf8(name, name_len) }) else {
        return RStatus::InvalidArgument;
    };
    let Ok(locations) = (unsafe { utf8(locations, locations_len) }) else {
        return RStatus::InvalidArgument;
    };
    let locations = locations
        .split('\0')
        .filter(|location| !location.is_empty())
        .collect::<Vec<_>>();
    protect(context, |context| {
        context
            .register_namespace_module(name, &locations)
            .map_err(|message| {
                record_exception(context, "RuntimeError", message);
                RStatus::Exception
            })
    })
}

/// Registers immutable package data embedded in the entry object.
///
/// # Safety
/// Module/name/data storage must be readable for the declared lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_register_module_resource(
    context: *mut RimeraContext,
    module: *const u8,
    module_len: usize,
    name: *const u8,
    name_len: usize,
    data: *const u8,
    data_len: usize,
) -> RStatus {
    let Ok(module) = (unsafe { utf8(module, module_len) }) else {
        return RStatus::InvalidArgument;
    };
    let Ok(name) = (unsafe { utf8(name, name_len) }) else {
        return RStatus::InvalidArgument;
    };
    if data.is_null() && data_len != 0 {
        return RStatus::InvalidArgument;
    }
    let bytes = if data_len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(data, data_len) }
    };
    protect(context, |context| {
        context
            .register_module_resource(module, name, bytes)
            .map_err(|message| {
                record_exception(context, "RuntimeError", message);
                RStatus::Exception
            })
    })
}

/// Imports one statically resolved Python source module through its native
/// initializer and the authoritative managed module cache.
///
/// # Safety
/// Name storage must be readable, `initializer` must be a generated function
/// with the documented ABI, and `output` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_import_source(
    context: *mut RimeraContext,
    name: *const u8,
    name_len: usize,
    filename: *const u8,
    filename_len: usize,
    package: *const u8,
    package_len: usize,
    is_package: u8,
    initializer: RNativeModuleInitializer,
    output: *mut RValue,
) -> RStatus {
    let Ok(name) = (unsafe { utf8(name, name_len) }) else {
        return RStatus::InvalidArgument;
    };
    let Ok(filename) = (unsafe { utf8(filename, filename_len) }) else {
        return RStatus::InvalidArgument;
    };
    let Ok(package) = (unsafe { utf8(package, package_len) }) else {
        return RStatus::InvalidArgument;
    };
    with_output(context, output, |context| {
        context.import_source(name, filename, package, is_package != 0, initializer)
    })
}

/// Creates or returns a statically resolved namespace package. `locations`
/// contains its ordered search roots separated by NUL bytes.
///
/// # Safety
/// Name/location storage must be readable and `output` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_import_namespace(
    context: *mut RimeraContext,
    name: *const u8,
    name_len: usize,
    locations: *const u8,
    locations_len: usize,
    output: *mut RValue,
) -> RStatus {
    let Ok(name) = (unsafe { utf8(name, name_len) }) else {
        return RStatus::InvalidArgument;
    };
    let Ok(locations) = (unsafe { utf8(locations, locations_len) }) else {
        return RStatus::InvalidArgument;
    };
    let locations = locations
        .split('\0')
        .filter(|location| !location.is_empty())
        .collect::<Vec<_>>();
    with_output(context, output, |context| {
        context.import_namespace(name, &locations)
    })
}

/// Imports one name from a managed module, optionally falling back to a
/// statically linked child-module initializer.
///
/// # Safety
/// Name storage and `output` follow the common ABI pointer contract. A non-null
/// initializer must use `RNativeModuleInitializer`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_import_from(
    context: *mut RimeraContext,
    module: *const RValue,
    name: *const u8,
    name_len: usize,
    filename: *const u8,
    filename_len: usize,
    package: *const u8,
    package_len: usize,
    is_package: u8,
    initializer: *const c_void,
    locations: *const u8,
    locations_len: usize,
    output: *mut RValue,
) -> RStatus {
    if module.is_null() {
        return RStatus::InvalidArgument;
    }
    let Ok(name) = (unsafe { utf8(name, name_len) }) else {
        return RStatus::InvalidArgument;
    };
    let metadata = if initializer.is_null() {
        ("", "")
    } else {
        let Ok(filename) = (unsafe { utf8(filename, filename_len) }) else {
            return RStatus::InvalidArgument;
        };
        let Ok(package) = (unsafe { utf8(package, package_len) }) else {
            return RStatus::InvalidArgument;
        };
        (filename, package)
    };
    let initializer = if initializer.is_null() {
        None
    } else {
        // SAFETY: generated code passes only addresses of linked module
        // initializer exports with the documented ABI.
        Some(unsafe { std::mem::transmute::<*const c_void, RNativeModuleInitializer>(initializer) })
    };
    let Ok(locations) = (unsafe { utf8(locations, locations_len) }) else {
        return RStatus::InvalidArgument;
    };
    let locations = locations
        .split('\0')
        .filter(|location| !location.is_empty())
        .collect::<Vec<_>>();
    with_output(context, output, |context| {
        context.import_from(
            unsafe { *module },
            name,
            metadata.0,
            metadata.1,
            is_package != 0,
            initializer,
            &locations,
        )
    })
}

/// Publishes a module's `__all__` names, or its public namespace names, into
/// the current module globals.
///
/// # Safety
/// `module` must point to a readable `RValue`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_import_star(
    context: *mut RimeraContext,
    module: *const RValue,
) -> RStatus {
    if module.is_null() {
        return RStatus::InvalidArgument;
    }
    protect(context, |context| {
        match context.import_star(unsafe { *module }) {
            Ok(_) => Ok(()),
            Err(_) if context.raised.is_some() => Err(RStatus::Exception),
            Err(message) => {
                let exception_type = if message.contains("must be str") {
                    "TypeError"
                } else {
                    "ImportError"
                };
                record_exception(context, exception_type, message);
                Err(RStatus::Exception)
            }
        }
    })
}

/// Reads a module global, falling back to the builtins dictionary.
///
/// # Safety
/// Name storage must be readable and `output` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_global_get(
    context: *mut RimeraContext,
    name: *const u8,
    name_len: usize,
    output: *mut RValue,
) -> RStatus {
    if output.is_null() || (name.is_null() && name_len != 0) {
        return RStatus::InvalidArgument;
    }
    let name_bytes = if name_len == 0 {
        &[]
    } else {
        // SAFETY: the ABI requires `name_len` readable bytes.
        unsafe { std::slice::from_raw_parts(name, name_len) }
    };
    let callsite = name as usize;
    // Successful global/builtin lookup is read-only. Lazy kernel/builtin
    // construction enforces its own allocation contracts, so the hot hit path
    // must not rescan the complete managed heap.
    protect_bookkeeping(context, |context| {
        context
            .initialize_kernel()
            .map_err(|_| RStatus::Exception)?;
        if let Some(value) = context.cached_global_lookup(callsite, name_bytes) {
            unsafe { output.write(value) };
            return Ok(());
        }
        let name = std::str::from_utf8(name_bytes).map_err(|_| RStatus::InvalidArgument)?;
        let globals = context.globals();
        if let Some(namespace) = globals
            && let Some(HeapObject::Dictionary(dictionary)) = context.heap.get(namespace)
            && let Some((entry_index, value)) = dictionary.get_index(name)
        {
            context.remember_global_lookup(callsite, namespace, entry_index);
            unsafe { output.write(value) };
            return Ok(());
        }
        if let Some(value) = context.execution_global(name).map_err(|message| {
            if context.raised.is_none() {
                record_exception(context, "NameError", message);
            }
            RStatus::Exception
        })? {
            unsafe { output.write(value) };
            return Ok(());
        }
        let value = context.execution_builtin(name).map_err(|message| {
            if context.raised.is_none() {
                record_exception(context, "RuntimeError", message);
            }
            RStatus::Exception
        })?;
        let Some(value) = value else {
            record_exception(
                context,
                "NameError",
                format!("name '{name}' is not defined"),
            );
            return Err(RStatus::Exception);
        };
        unsafe { output.write(value) };
        Ok(())
    })
}

/// Deletes a module global by UTF-8 name.
///
/// # Safety
/// Name storage must remain readable for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_global_delete(
    context: *mut RimeraContext,
    name: *const u8,
    name_len: usize,
) -> RStatus {
    let Ok(name) = (unsafe { utf8(name, name_len) }) else {
        return RStatus::InvalidArgument;
    };
    protect(context, |context| {
        context
            .initialize_kernel()
            .map_err(|_| RStatus::Exception)?;
        let globals = context.globals().ok_or(RStatus::InvalidArgument)?;
        let globals = context
            .dictionary_storage(globals)
            .ok_or(RStatus::InvalidArgument)?;
        context.namespace_delete(globals, name).map_err(|_| {
            record_exception(
                context,
                "NameError",
                format!("name '{name}' is not defined"),
            );
            RStatus::Exception
        })
    })
}

/// Creates an ordered native namespace for a class body.
///
/// # Safety
/// `output` must be writable for one `RValue`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_namespace_new(
    context: *mut RimeraContext,
    output: *mut RValue,
) -> RStatus {
    with_output(context, output, RimeraContext::namespace_new)
}

/// Ensures `__annotations__` exists in globals or a prepared class namespace.
/// Existing mappings are preserved.
///
/// # Safety
/// When non-null, `namespace` must point to a readable live `RValue`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_annotations_ensure(
    context: *mut RimeraContext,
    namespace: *const RValue,
) -> RStatus {
    let namespace = if namespace.is_null() {
        None
    } else {
        // SAFETY: non-null was checked and the input is read-only for this call.
        Some(unsafe { *namespace })
    };
    protect(context, |context| {
        context.ensure_annotations(namespace).map_err(|message| {
            if context.raised.is_none() {
                record_exception(context, "TypeError", message);
            }
            RStatus::Exception
        })
    })
}

/// Stores one named value in a class namespace.
///
/// # Safety
/// Name and value storage must remain readable for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_namespace_set(
    context: *mut RimeraContext,
    namespace: *const RValue,
    name: *const u8,
    name_len: usize,
    value: *const RValue,
) -> RStatus {
    if namespace.is_null() || value.is_null() {
        return RStatus::InvalidArgument;
    }
    let Ok(name) = (unsafe { utf8(name, name_len) }) else {
        return RStatus::InvalidArgument;
    };
    let namespace = unsafe { *namespace };
    let value = unsafe { *value };
    protect(context, |context| {
        context
            .with_temporary_roots(&[namespace, value], |context| {
                context.namespace_set(namespace, name, value)
            })
            .map_err(|message| {
                if context.raised.is_none() {
                    record_exception(context, "TypeError", message);
                }
                RStatus::Exception
            })
    })
}

/// Reads a value from a class namespace while the class body is executing.
///
/// # Safety
/// Namespace and output storage must be readable and writable for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_namespace_get(
    context: *mut RimeraContext,
    namespace: *const RValue,
    name: *const u8,
    name_len: usize,
    output: *mut RValue,
) -> RStatus {
    if namespace.is_null() {
        return RStatus::InvalidArgument;
    }
    let Ok(name) = (unsafe { utf8(name, name_len) }) else {
        return RStatus::InvalidArgument;
    };
    let namespace = unsafe { *namespace };
    with_output(context, output, |context| {
        context.namespace_get(namespace, name)
    })
}

/// Resolves a name from a prepared class namespace, then globals and builtins.
///
/// # Safety
/// Namespace storage and output must be valid for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_class_name_get(
    context: *mut RimeraContext,
    namespace: *const RValue,
    name: *const u8,
    name_len: usize,
    output: *mut RValue,
) -> RStatus {
    if namespace.is_null() || output.is_null() {
        return RStatus::InvalidArgument;
    }
    let Ok(name) = (unsafe { utf8(name, name_len) }) else {
        return RStatus::InvalidArgument;
    };
    let namespace = unsafe { *namespace };
    protect(context, |context| {
        context
            .initialize_kernel()
            .map_err(|_| RStatus::Exception)?;
        let value = context.class_name_get(namespace, name).map_err(|message| {
            if context.raised.is_none() {
                record_exception(context, "NameError", message);
            }
            RStatus::Exception
        })?;
        unsafe { output.write(value) };
        Ok(())
    })
}

/// Resolves a class-body free name from the prepared namespace and then an
/// enclosing closure cell.
///
/// # Safety
/// Namespace, cell, name, and output storage must be valid for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_class_free_get(
    context: *mut RimeraContext,
    namespace: *const RValue,
    cell: *const RValue,
    name: *const u8,
    name_len: usize,
    output: *mut RValue,
) -> RStatus {
    if namespace.is_null() || cell.is_null() || output.is_null() {
        return RStatus::InvalidArgument;
    }
    let Ok(name) = (unsafe { utf8(name, name_len) }) else {
        return RStatus::InvalidArgument;
    };
    let (namespace, cell) = unsafe { (*namespace, *cell) };
    with_output(context, output, |context| {
        context.initialize_kernel()?;
        context
            .class_free_get(namespace, cell, name)
            .inspect_err(|message| {
                if context.raised.is_none() {
                    record_exception(context, "NameError", message.clone());
                }
            })
    })
}

/// Deletes a name from a class namespace while the class body is executing.
///
/// # Safety
/// Namespace storage must remain readable for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_namespace_delete(
    context: *mut RimeraContext,
    namespace: *const RValue,
    name: *const u8,
    name_len: usize,
) -> RStatus {
    if namespace.is_null() {
        return RStatus::InvalidArgument;
    }
    let Ok(name) = (unsafe { utf8(name, name_len) }) else {
        return RStatus::InvalidArgument;
    };
    let namespace = unsafe { *namespace };
    protect(context, |context| {
        context
            .namespace_delete(namespace, name)
            .map_err(|message| {
                if context.raised.is_none() {
                    record_exception(context, "NameError", message);
                }
                RStatus::Exception
            })
    })
}

/// Reads a Python attribute through the native object model.
///
/// # Safety
/// Receiver storage must be readable and `output` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_attr_get(
    context: *mut RimeraContext,
    receiver: *const RValue,
    name: *const u8,
    name_len: usize,
    output: *mut RValue,
) -> RStatus {
    if receiver.is_null() {
        return RStatus::InvalidArgument;
    }
    let Ok(name) = (unsafe { utf8(name, name_len) }) else {
        return RStatus::InvalidArgument;
    };
    let receiver = unsafe { *receiver };
    with_output(context, output, |context| {
        context.with_temporary_roots(&[receiver], |context| context.attribute_get(receiver, name))
    })
}

/// Resolves and descriptor-binds one context-manager special method from the
/// receiver's type/MRO. This performs lookup only; generated MIR still owns
/// enter/body/exit ordering and cleanup.
///
/// # Safety
/// Receiver storage must be readable and `output` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_special_method_get(
    context: *mut RimeraContext,
    receiver: *const RValue,
    name: *const u8,
    name_len: usize,
    output: *mut RValue,
) -> RStatus {
    if receiver.is_null() {
        return RStatus::InvalidArgument;
    }
    let Ok(name) = (unsafe { utf8(name, name_len) }) else {
        return RStatus::InvalidArgument;
    };
    let receiver = unsafe { *receiver };
    with_output(context, output, |context| {
        context.with_temporary_roots(&[receiver], |context| {
            let Some(method) = context.special_method(receiver, name)? else {
                let receiver_type = context.type_of(receiver)?;
                let type_name = context.type_name(receiver_type);
                let message = if name == "__aexit__" {
                    format!(
                        "'{type_name}' object does not support the asynchronous context manager protocol (missed __aexit__ method)"
                    )
                } else if name == "__aenter__" {
                    format!("'{type_name}' object does not support the asynchronous context manager protocol")
                } else if name == "__exit__" {
                    format!(
                        "'{type_name}' object does not support the context manager protocol (missed __exit__ method)"
                    )
                } else {
                    format!("'{type_name}' object does not support the context manager protocol")
                };
                return Err(message);
            };
            Ok(method)
        })
    })
}

/// Constructs the state used by explicit two-argument `super(type, receiver)`.
///
/// # Safety
/// `start_type`, `receiver`, and `output` must point to readable or writable
/// ABI values for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_super_new(
    context: *mut RimeraContext,
    start_type: *const RValue,
    receiver: *const RValue,
    output: *mut RValue,
) -> RStatus {
    if start_type.is_null() || receiver.is_null() || output.is_null() {
        return RStatus::InvalidArgument;
    }
    // SAFETY: null pointers were rejected and both input values are readable.
    let (start_type, receiver) = unsafe { (*start_type, *receiver) };
    with_output(context, output, |context| {
        context.new_super(start_type, receiver)
    })
}

/// Writes a Python attribute through the native object model.
///
/// # Safety
/// Receiver and value storage must remain readable for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_attr_set(
    context: *mut RimeraContext,
    receiver: *const RValue,
    name: *const u8,
    name_len: usize,
    value: *const RValue,
) -> RStatus {
    if receiver.is_null() || value.is_null() {
        return RStatus::InvalidArgument;
    }
    let Ok(name) = (unsafe { utf8(name, name_len) }) else {
        return RStatus::InvalidArgument;
    };
    let receiver = unsafe { *receiver };
    let value = unsafe { *value };
    protect(context, |context| {
        context
            .with_temporary_roots(&[receiver, value], |context| {
                context.attribute_set(receiver, name, value)
            })
            .map_err(|message| {
                if context.raised.is_none() {
                    record_exception(context, "AttributeError", message);
                }
                RStatus::Exception
            })
    })
}

/// Deletes a Python attribute through the native object model.
///
/// # Safety
/// Receiver storage must remain readable for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_attr_delete(
    context: *mut RimeraContext,
    receiver: *const RValue,
    name: *const u8,
    name_len: usize,
) -> RStatus {
    if receiver.is_null() {
        return RStatus::InvalidArgument;
    }
    let Ok(name) = (unsafe { utf8(name, name_len) }) else {
        return RStatus::InvalidArgument;
    };
    let receiver = unsafe { *receiver };
    protect(context, |context| {
        context
            .with_temporary_roots(&[receiver], |context| {
                context.attribute_delete(receiver, name)
            })
            .map_err(|message| {
                if context.raised.is_none() {
                    record_exception(context, "AttributeError", message);
                }
                RStatus::Exception
            })
    })
}

/// Merges one mapping into an existing native dictionary for `{**mapping}`.
///
/// Unlike `dict.update`, this accepts mappings only; iterable-of-pairs fallback
/// is intentionally not part of dictionary-display semantics.
///
/// # Safety
/// `dictionary` and `source` must be readable values owned by the context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_dictionary_merge(
    context: *mut RimeraContext,
    dictionary: *const RValue,
    source: *const RValue,
) -> RStatus {
    if dictionary.is_null() || source.is_null() {
        return RStatus::InvalidArgument;
    }
    let (dictionary, source) = unsafe { (*dictionary, *source) };
    protect(context, |context| {
        match call::dict_merge_mapping_source(context, dictionary, source) {
            Ok(()) => Ok(()),
            Err(_) if context.raised.is_some() => Err(RStatus::Exception),
            Err(message) => {
                record_exception(context, "TypeError", message);
                Err(RStatus::Exception)
            }
        }
    })
}

/// Creates a traced source-ordered call-argument accumulator.
///
/// # Safety
/// `callable` must be readable and `output` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_call_arguments_new(
    context: *mut RimeraContext,
    callable: *const RValue,
    output: *mut RValue,
) -> RStatus {
    if callable.is_null() {
        return RStatus::InvalidArgument;
    }
    let callable = unsafe { *callable };
    with_output(context, output, |context| {
        call::call_arguments_new(context, callable)
    })
}

/// Appends one already-evaluated source call part, expanding `*`/`**` through
/// the generic iteration/mapping protocols before later source parts execute.
///
/// # Safety
/// `arguments` and `value` must be readable. `name`, when non-null, must point
/// to UTF-8 bytes for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_call_argument_add(
    context: *mut RimeraContext,
    arguments: *const RValue,
    kind: u8,
    name: *const u8,
    name_len: usize,
    value: *const RValue,
) -> RStatus {
    if arguments.is_null() || value.is_null() || (name.is_null() && name_len != 0) {
        return RStatus::InvalidArgument;
    }
    let Ok(kind) = RCallArgumentKind::try_from(kind) else {
        return RStatus::InvalidArgument;
    };
    let name = if name.is_null() {
        None
    } else {
        let Ok(name) = (unsafe { utf8(name, name_len) }) else {
            return RStatus::InvalidArgument;
        };
        Some(name)
    };
    let (arguments, value) = unsafe { (*arguments, *value) };
    protect(context, |context| {
        match call::call_argument_add(context, arguments, kind, name, value) {
            Ok(()) => Ok(()),
            Err(_) if context.raised.is_some() => Err(RStatus::Exception),
            Err(message) => {
                record_exception(context, "TypeError", message);
                Err(RStatus::Exception)
            }
        }
    })
}

/// Invokes a source-ordered prepared call through the authoritative binder.
///
/// # Safety
/// Inputs must be readable and `output` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_call_prepared(
    context: *mut RimeraContext,
    callable: *const RValue,
    arguments: *const RValue,
    output: *mut RValue,
) -> RStatus {
    if callable.is_null() || arguments.is_null() {
        return RStatus::InvalidArgument;
    }
    let (callable, arguments) = unsafe { (*callable, *arguments) };
    with_output(context, output, |context| {
        call::invoke_prepared(context, callable, arguments)
    })
}

/// Invokes a positional managed call from generated code after the caller has
/// published the callable and argument values in its GC root frame. Exact
/// Rimera functions bypass descriptor construction and redundant native-root
/// copies; dynamic callables preserve the full Python dispatcher fallback.
///
/// # Safety
/// `callable` and `output` must be live. `positional` must reference
/// `positional_len` readable rooted values, or may be null when the length is
/// zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_call_positional_rooted(
    context: *mut RimeraContext,
    callable: *const RValue,
    positional: *const RValue,
    positional_len: usize,
    output: *mut RValue,
) -> RStatus {
    if callable.is_null() || output.is_null() || (positional.is_null() && positional_len != 0) {
        return RStatus::InvalidArgument;
    }
    let callable = unsafe { *callable };
    let positional = if positional_len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(positional, positional_len) }
    };
    protect_bookkeeping(context, |context| {
        match call::invoke_rooted_positional(context, callable, positional) {
            Ok(value) => {
                unsafe { output.write(value) };
                Ok(())
            }
            Err(_) if context.raised.is_some() => Err(RStatus::Exception),
            Err(message) => {
                let exception_type = if message == "managed heap limit exceeded" {
                    "MemoryError"
                } else {
                    "TypeError"
                };
                record_exception(context, exception_type, message);
                Err(RStatus::Exception)
            }
        }
    })
}

/// Attempts an allocation-free exact-positional call through a compiler-proven
/// non-suspending coroutine entry. On a miss, `matched` is false and no Python
/// call has occurred, so generated code may safely use the ordinary lazy call
/// and await protocol with the same already-evaluated operands.
///
/// # Safety
/// `callable`, `output`, and `matched` must be writable/readable as indicated.
/// `positional` references `positional_len` caller-rooted values or is null when
/// the length is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_call_ready_coroutine_rooted(
    context: *mut RimeraContext,
    callable: *const RValue,
    positional: *const RValue,
    positional_len: usize,
    pure_only: u8,
    output: *mut RValue,
    matched: *mut RValue,
) -> RStatus {
    if callable.is_null()
        || output.is_null()
        || matched.is_null()
        || pure_only > 1
        || (positional.is_null() && positional_len != 0)
    {
        return RStatus::InvalidArgument;
    }
    let callable = unsafe { *callable };
    let positional = if positional_len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(positional, positional_len) }
    };
    protect_bookkeeping(
        context,
        |context| match call::invoke_ready_coroutine_rooted(
            context,
            callable,
            positional,
            pure_only != 0,
        ) {
            Ok(Some(value)) => {
                unsafe {
                    output.write(value);
                    matched.write(RValue::boolean(true));
                }
                Ok(())
            }
            Ok(None) => {
                unsafe {
                    output.write(RValue::NONE);
                    matched.write(RValue::boolean(false));
                }
                Ok(())
            }
            Err(_) if context.raised.is_some() => Err(RStatus::Exception),
            Err(message) => {
                let exception_type = if message == "managed heap limit exceeded" {
                    "MemoryError"
                } else {
                    "TypeError"
                };
                record_exception(context, exception_type, message);
                Err(RStatus::Exception)
            }
        },
    )
}

/// Invokes a managed callable using Python argument binding.
///
/// # Safety
/// The callable, argument descriptor, its slices, and output must be live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_call(
    context: *mut RimeraContext,
    callable: *const RValue,
    arguments: *const RCallArguments,
    output: *mut RValue,
) -> RStatus {
    if callable.is_null() || arguments.is_null() || output.is_null() {
        return RStatus::InvalidArgument;
    }
    let callable = unsafe { *callable };
    let arguments = unsafe { *arguments };
    if (arguments.positional.is_null() && arguments.positional_len != 0)
        || (arguments.keywords.is_null() && arguments.keyword_len != 0)
    {
        return RStatus::InvalidArgument;
    }
    let positional = if arguments.positional_len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(arguments.positional, arguments.positional_len) }
    };
    let keyword_arguments = if arguments.keyword_len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(arguments.keywords, arguments.keyword_len) }
    };
    let mut keywords = Vec::with_capacity(keyword_arguments.len());
    for keyword in keyword_arguments {
        let Ok(name) = (unsafe { utf8(keyword.name, keyword.name_len) }) else {
            return RStatus::InvalidArgument;
        };
        keywords.push((name.to_owned(), keyword.value));
    }
    // `call::invoke` and every managed allocation/mutation it reaches enforce
    // their own heap-growth contracts. Re-scanning the complete heap after
    // every successful Python call is redundant and destroys small-call
    // performance, so this outer dispatch uses the bookkeeping guard.
    protect_bookkeeping(context, |context| {
        match call::invoke(context, callable, positional, &keywords) {
            Ok(value) => {
                unsafe { output.write(value) };
                Ok(())
            }
            Err(_) if context.raised.is_some() => Err(RStatus::Exception),
            Err(message) => {
                let exception_type = if message == "managed heap limit exceeded" {
                    "MemoryError"
                } else {
                    "TypeError"
                };
                record_exception(context, exception_type, message);
                Err(RStatus::Exception)
            }
        }
    })
}

/// Creates an exception instance of a built-in exception type.
///
/// # Safety
/// Type and argument values must be readable and `output` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_exception_new(
    context: *mut RimeraContext,
    exception_type: *const RValue,
    arguments: *const RValue,
    argument_len: usize,
    output: *mut RValue,
) -> RStatus {
    if exception_type.is_null() || (arguments.is_null() && argument_len != 0) {
        return RStatus::InvalidArgument;
    }
    let exception_type = unsafe { *exception_type };
    let arguments = if argument_len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(arguments, argument_len) }
    };
    with_output(context, output, |context| {
        context.initialize_kernel()?;
        context.new_exception(exception_type, arguments)
    })
}

/// Raises an exception with an optional explicit cause.
///
/// A null cause with `suppress_context != 0` implements `raise X from None`.
///
/// # Safety
/// Non-null value pointers must be readable and owned by the context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_raise(
    context: *mut RimeraContext,
    exception: *const RValue,
    cause: *const RValue,
    suppress_context: u8,
) -> RStatus {
    if exception.is_null() {
        return RStatus::InvalidArgument;
    }
    let exception = unsafe { *exception };
    let cause_value = (!cause.is_null()).then(|| unsafe { *cause });
    let cause_is_none = cause_value == Some(RValue::NONE);
    let cause = cause_value.filter(|value| *value != RValue::NONE);
    protect(context, |context| {
        let exception = context
            .normalize_raise_operand(exception)
            .map_err(|message| {
                record_exception(context, "TypeError", message);
                RStatus::Exception
            })?;
        let cause = context
            .with_temporary_roots(&[exception], |context| {
                cause
                    .map(|value| context.normalize_raise_operand(value))
                    .transpose()
            })
            .map_err(|message| {
                record_exception(context, "TypeError", message);
                RStatus::Exception
            })?;
        context
            .raise_value(exception, cause, suppress_context != 0 || cause_is_none)
            .map_err(|message| {
                record_exception(context, "TypeError", message);
                RStatus::Exception
            })?;
        Err(RStatus::Exception)
    })
}

/// Reads the currently raised exception without entering a handler.
///
/// # Safety
/// `output` must be writable and the context live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_exception_active(
    context: *mut RimeraContext,
    output: *mut RValue,
) -> RStatus {
    if output.is_null() {
        return RStatus::InvalidArgument;
    }
    protect(context, |context| {
        let Some(exception) = context.raised else {
            context.fail("no exception is currently raised");
            return Err(RStatus::InvalidArgument);
        };
        unsafe { output.write(exception) };
        Ok(())
    })
}

/// Re-raises the innermost handled exception.
///
/// # Safety
/// `context` must be a live context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_reraise(context: *mut RimeraContext) -> RStatus {
    protect(context, |context| match context.reraise() {
        Ok(()) => Err(RStatus::Exception),
        Err(message) => {
            record_exception(context, "RuntimeError", message);
            Err(RStatus::Exception)
        }
    })
}

/// Continues propagating the currently raised exception unchanged.
///
/// # Safety
/// `context` must be live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_exception_propagate(context: *mut RimeraContext) -> RStatus {
    protect(context, |context| {
        if context.raised.is_some() {
            Err(RStatus::Exception)
        } else {
            context.fail("no exception is available to propagate");
            Err(RStatus::InvalidArgument)
        }
    })
}

/// Transfers the active raised exception into the handled stack.
///
/// # Safety
/// `output` must be writable and the context live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_handler_enter(
    context: *mut RimeraContext,
    output: *mut RValue,
) -> RStatus {
    if output.is_null() {
        return RStatus::InvalidArgument;
    }
    protect(context, |context| match context.handler_enter() {
        Ok(exception) => {
            unsafe { output.write(exception) };
            Ok(())
        }
        Err(message) => {
            context.fail(message);
            Err(RStatus::InvalidArgument)
        }
    })
}

/// Leaves the innermost active exception handler.
///
/// # Safety
/// `context` must be live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_handler_leave(context: *mut RimeraContext) -> RStatus {
    protect(context, |context| {
        context.handler_leave().map_err(|message| {
            context.fail(message);
            RStatus::InvalidArgument
        })
    })
}

/// Tests whether an exception is an instance of a built-in exception type.
///
/// # Safety
/// Inputs must be readable and `output` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_exception_matches(
    context: *mut RimeraContext,
    exception: *const RValue,
    expected_type: *const RValue,
    output: *mut RValue,
) -> RStatus {
    if exception.is_null() || expected_type.is_null() {
        return RStatus::InvalidArgument;
    }
    let (exception, expected_type) = unsafe { (*exception, *expected_type) };
    with_output(context, output, |context| {
        context
            .exception_matches(exception, expected_type)
            .map(RValue::boolean)
    })
}

/// Splits an exception or exception group by a built-in exception type.
///
/// Empty matched or remainder partitions are returned as `None`.
///
/// # Safety
/// Inputs must be readable and both outputs writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_exception_split(
    context: *mut RimeraContext,
    exception: *const RValue,
    expected_type: *const RValue,
    output: *mut RValue,
) -> RStatus {
    if exception.is_null() || expected_type.is_null() || output.is_null() {
        return RStatus::InvalidArgument;
    }
    let (exception, expected_type) = unsafe { (*exception, *expected_type) };
    with_output(context, output, |context| {
        let (matched, rest) = context.split_exception(exception, expected_type)?;
        let values = [
            matched.unwrap_or(RValue::NONE),
            rest.unwrap_or(RValue::NONE),
        ];
        context.with_temporary_roots(&values, |context| operations::value_array(context, &values))
    })
}

/// Makes an exception value the currently raised exception.
///
/// This is used by native `except*` landing blocks before entering a matched
/// subgroup handler.
///
/// # Safety
/// `exception` must be readable and owned by the context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_exception_set_active(
    context: *mut RimeraContext,
    exception: *const RValue,
) -> RStatus {
    if exception.is_null() {
        return RStatus::InvalidArgument;
    }
    let exception = unsafe { *exception };
    protect(context, |context| {
        if !matches!(context.heap.get(exception), Some(HeapObject::Exception(_))) {
            record_exception(
                context,
                "TypeError",
                "active exception must be an exception instance".to_owned(),
            );
            return Err(RStatus::Exception);
        }
        context.raised = Some(exception);
        Ok(())
    })
}

/// Merges the active exception with an unhandled `except*` remainder.
///
/// # Safety
/// `remainder` must be readable and owned by the context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_exception_merge_active(
    context: *mut RimeraContext,
    remainder: *const RValue,
) -> RStatus {
    if remainder.is_null() {
        return RStatus::InvalidArgument;
    }
    let remainder = unsafe { *remainder };
    protect(context, |context| {
        context
            .merge_active_exception_group(remainder)
            .map_err(|message| {
                record_exception(context, "RuntimeError", message);
                RStatus::Exception
            })
    })
}

/// Combines two optional exception values into one propagated value.
///
/// `None` acts as the empty partition.
///
/// # Safety
/// Inputs must be readable and `output` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_exception_combine(
    context: *mut RimeraContext,
    left: *const RValue,
    right: *const RValue,
    output: *mut RValue,
) -> RStatus {
    if left.is_null() || right.is_null() {
        return RStatus::InvalidArgument;
    }
    let (left, right) = unsafe { (*left, *right) };
    with_output(context, output, |context| {
        context.combine_exceptions(left, right)
    })
}

/// Clears the currently raised exception after an `except*` landing block
/// saves it for deferred merging.
///
/// # Safety
/// `context` must be live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_exception_clear_active(context: *mut RimeraContext) -> RStatus {
    protect(context, |context| {
        context.raised = None;
        Ok(())
    })
}

/// Appends the module frame to the active exception traceback without linking
/// function/generator activation-introspection machinery into module-only
/// artifacts.
///
/// # Safety
/// Filename and function slices must remain readable for the call.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn rimera_traceback_append_module(
    context: *mut RimeraContext,
    filename: *const u8,
    filename_len: usize,
    function: *const u8,
    function_len: usize,
    line: u32,
    column: u32,
) -> RStatus {
    let (Ok(filename), Ok(function)) =
        (unsafe { (utf8(filename, filename_len), utf8(function, function_len)) })
    else {
        return RStatus::InvalidArgument;
    };
    protect(context, |context| {
        match context.attach_module_traceback(filename, function, line, column) {
            Ok(()) => Ok(()),
            Err(message) if message == "managed heap limit exceeded" => {
                record_exception(context, "MemoryError", message);
                Err(RStatus::Exception)
            }
            Err(message) => {
                context.fail(message);
                Err(RStatus::InvalidArgument)
            }
        }
    })
}

/// Appends one immutable function/generator frame to the active exception
/// traceback.
///
/// # Safety
/// Filename and function slices must remain readable for the call.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn rimera_traceback_append(
    context: *mut RimeraContext,
    filename: *const u8,
    filename_len: usize,
    function: *const u8,
    function_len: usize,
    line: u32,
    column: u32,
) -> RStatus {
    let (Ok(filename), Ok(function)) =
        (unsafe { (utf8(filename, filename_len), utf8(function, function_len)) })
    else {
        return RStatus::InvalidArgument;
    };
    protect(context, |context| {
        match context.attach_traceback(filename, function, line, column) {
            Ok(()) => Ok(()),
            Err(message) if message == "managed heap limit exceeded" => {
                record_exception(context, "MemoryError", message);
                Err(RStatus::Exception)
            }
            Err(message) => {
                context.fail(message);
                Err(RStatus::InvalidArgument)
            }
        }
    })
}

/// Registers a generated stack root frame.
///
/// # Safety
/// The context and frame must stay live until a matching pop.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_roots_push(
    context: *mut RimeraContext,
    frame: *mut RRootFrame,
) -> RStatus {
    if frame.is_null() {
        return RStatus::InvalidArgument;
    }
    protect_bookkeeping(context, |context| {
        // SAFETY: the frame is live and writable for its registration period.
        unsafe { (*frame).previous = context.roots };
        context.roots = frame;
        Ok(())
    })
}

/// Removes the most recently registered root frame.
///
/// # Safety
/// Both pointers must be live and `frame` must be the active frame.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_roots_pop(
    context: *mut RimeraContext,
    frame: *mut RRootFrame,
) -> RStatus {
    if frame.is_null() {
        return RStatus::InvalidArgument;
    }
    protect_bookkeeping(context, |context| {
        if context.roots != frame {
            context.fail("root frames must be removed in stack order");
            return Err(RStatus::InvalidArgument);
        }
        // SAFETY: the registered frame remains live until this call returns.
        context.roots = unsafe { (*frame).previous };
        // SAFETY: the frame is writable by its owner.
        unsafe { (*frame).previous = ptr::null_mut() };
        Ok(())
    })
}

/// Constructs a Python integer from decimal UTF-8.
///
/// # Safety
/// `bytes` must describe `len` readable bytes and `output` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_int_from_decimal(
    context: *mut RimeraContext,
    bytes: *const u8,
    len: usize,
    output: *mut RValue,
) -> RStatus {
    if bytes.is_null() && len != 0 {
        return RStatus::InvalidArgument;
    }
    let bytes = if len == 0 {
        &[]
    } else {
        // SAFETY: non-null was checked and the caller supplies `len` bytes.
        unsafe { std::slice::from_raw_parts(bytes, len) }
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return RStatus::InvalidArgument;
    };
    with_output(context, output, |context| {
        operations::int_from_decimal(context, text)
    })
}

/// Constructs a managed UTF-8 string.
///
/// # Safety
/// `bytes` must describe `len` readable bytes and `output` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_string_new(
    context: *mut RimeraContext,
    bytes: *const u8,
    len: usize,
    output: *mut RValue,
) -> RStatus {
    if bytes.is_null() && len != 0 {
        return RStatus::InvalidArgument;
    }
    let bytes = if len == 0 {
        &[]
    } else {
        // SAFETY: non-null was checked and the caller supplies `len` bytes.
        unsafe { std::slice::from_raw_parts(bytes, len) }
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return RStatus::InvalidArgument;
    };
    with_output(context, output, |context| operations::string(context, text))
}

/// Constructs a managed Python float from its IEEE-754 bit pattern.
///
/// # Safety
/// `output` must be writable for one value owned by `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_float_new(
    context: *mut RimeraContext,
    bits: u64,
    output: *mut RValue,
) -> RStatus {
    with_output(context, output, |context| {
        operations::float(context, f64::from_bits(bits))
    })
}

/// Constructs a managed complex value from IEEE-754 component bit patterns.
///
/// # Safety
/// `output` must be writable for one value owned by `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_complex_new(
    context: *mut RimeraContext,
    real_bits: u64,
    imag_bits: u64,
    output: *mut RValue,
) -> RStatus {
    with_output(context, output, |context| {
        operations::complex(
            context,
            f64::from_bits(real_bits),
            f64::from_bits(imag_bits),
        )
    })
}

/// Constructs immutable Python bytes.
///
/// # Safety
/// `bytes` must describe `len` readable bytes and `output` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_bytes_new(
    context: *mut RimeraContext,
    bytes: *const u8,
    len: usize,
    output: *mut RValue,
) -> RStatus {
    if bytes.is_null() && len != 0 {
        return RStatus::InvalidArgument;
    }
    let bytes = if len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(bytes, len) }
    };
    with_output(context, output, |context| operations::bytes(context, bytes))
}

/// Constructs mutable Python bytearray storage.
///
/// # Safety
/// `bytes` must describe `len` readable bytes and `output` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_bytearray_new(
    context: *mut RimeraContext,
    bytes: *const u8,
    len: usize,
    output: *mut RValue,
) -> RStatus {
    if bytes.is_null() && len != 0 {
        return RStatus::InvalidArgument;
    }
    let bytes = if len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(bytes, len) }
    };
    with_output(context, output, |context| {
        operations::bytearray(context, bytes)
    })
}

/// Creates a native slice value. Null inputs represent omitted bounds.
///
/// # Safety
/// Non-null inputs and `output` must reference live writable storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_slice_new(
    context: *mut RimeraContext,
    start: *const RValue,
    stop: *const RValue,
    step: *const RValue,
    output: *mut RValue,
) -> RStatus {
    let start = (!start.is_null()).then(|| unsafe { *start });
    let stop = (!stop.is_null()).then(|| unsafe { *stop });
    let step = (!step.is_null()).then(|| unsafe { *step });
    with_output(context, output, |context| {
        operations::slice(context, start, stop, step)
    })
}

/// Creates a live view over a native dictionary. `kind` is 0 for keys, 1 for
/// values, and 2 for items.
///
/// # Safety
/// `dictionary` and `output` must reference live values owned by `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_dictionary_view_new(
    context: *mut RimeraContext,
    dictionary: *const RValue,
    kind: u8,
    output: *mut RValue,
) -> RStatus {
    if dictionary.is_null() {
        return RStatus::InvalidArgument;
    }
    let kind = match kind {
        0 => DictionaryViewKind::Keys,
        1 => DictionaryViewKind::Values,
        2 => DictionaryViewKind::Items,
        _ => return RStatus::InvalidArgument,
    };
    let dictionary = unsafe { *dictionary };
    with_output(context, output, |context| {
        operations::dictionary_view(context, dictionary, kind)
    })
}

/// Releases a native memoryview's exporter reference. Releasing an already
/// released view succeeds without changing the exporter count.
///
/// # Safety
/// `view` must point to a live value owned by `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_memoryview_release(
    context: *mut RimeraContext,
    view: *const RValue,
) -> RStatus {
    if view.is_null() {
        return RStatus::InvalidArgument;
    }
    let view = unsafe { *view };
    protect(context, |context| {
        match operations::memoryview_release(context, view) {
            Ok(()) => Ok(()),
            Err(error) => {
                context.fail(error);
                Err(RStatus::Exception)
            }
        }
    })
}

/// Creates an internal managed array of values for closure environments and
/// future container storage.
///
/// # Safety
/// `values` must describe `len` readable values and `output` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_value_array_new(
    context: *mut RimeraContext,
    values: *const RValue,
    len: usize,
    output: *mut RValue,
) -> RStatus {
    if values.is_null() && len != 0 {
        return RStatus::InvalidArgument;
    }
    let values = if len == 0 {
        &[]
    } else {
        // SAFETY: non-null was checked and the caller supplies `len` values.
        unsafe { std::slice::from_raw_parts(values, len) }
    };
    with_output(context, output, |context| {
        operations::value_array(context, values)
    })
}

/// Creates an immutable managed tuple from `len` values.
///
/// # Safety
/// `values` must describe `len` readable values and `output` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_tuple_new(
    context: *mut RimeraContext,
    values: *const RValue,
    len: usize,
    output: *mut RValue,
) -> RStatus {
    if values.is_null() && len != 0 {
        return RStatus::InvalidArgument;
    }
    let values = if len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(values, len) }
    };
    with_output(context, output, |context| {
        context.with_temporary_roots(values, |context| {
            context.allocate(HeapObject::Tuple(values.to_vec().into_boxed_slice()))
        })
    })
}

/// Creates a managed list from `len` values.
///
/// # Safety
/// `values` must describe `len` readable values and `output` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_list_new(
    context: *mut RimeraContext,
    values: *const RValue,
    len: usize,
    output: *mut RValue,
) -> RStatus {
    if values.is_null() && len != 0 {
        return RStatus::InvalidArgument;
    }
    let values = if len == 0 {
        &[]
    } else {
        // SAFETY: non-null was checked and the caller supplies `len` values.
        unsafe { std::slice::from_raw_parts(values, len) }
    };
    with_output(context, output, |context| operations::list(context, values))
}

/// Appends one value to the exact list owned by a comprehension activation.
///
/// # Safety
/// `list` and `value` must be readable values owned by `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_list_append(
    context: *mut RimeraContext,
    list: *const RValue,
    value: *const RValue,
) -> RStatus {
    if list.is_null() || value.is_null() {
        return RStatus::InvalidArgument;
    }
    let (list, value) = unsafe { (*list, *value) };
    protect(context, |context| {
        match operations::list_append(context, list, value) {
            Ok(()) => Ok(()),
            Err(message) => {
                if context.raised.is_some() {
                    return Err(RStatus::Exception);
                }
                record_exception(context, "TypeError", message);
                Err(RStatus::Exception)
            }
        }
    })
}

/// Creates an insertion-ordered dictionary from parallel key/value arrays.
///
/// # Safety
/// `keys` and `values` each describe `len` readable values; `output` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_dict_new(
    context: *mut RimeraContext,
    keys: *const RValue,
    values: *const RValue,
    len: usize,
    output: *mut RValue,
) -> RStatus {
    if (keys.is_null() || values.is_null()) && len != 0 {
        return RStatus::InvalidArgument;
    }
    let keys = if len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(keys, len) }
    };
    let values = if len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(values, len) }
    };
    with_output(context, output, |context| {
        operations::dictionary(context, keys, values)
    })
}

/// Inserts or replaces one key/value pair in a comprehension dictionary.
///
/// # Safety
/// All input values must be readable and owned by `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_dictionary_insert(
    context: *mut RimeraContext,
    dictionary: *const RValue,
    key: *const RValue,
    value: *const RValue,
) -> RStatus {
    if dictionary.is_null() || key.is_null() || value.is_null() {
        return RStatus::InvalidArgument;
    }
    let (dictionary, key, value) = unsafe { (*dictionary, *key, *value) };
    protect(context, |context| {
        match operations::item_set(context, dictionary, key, value) {
            Ok(()) => Ok(()),
            Err(message) => {
                if context.raised.is_some() {
                    return Err(RStatus::Exception);
                }
                record_exception(context, "TypeError", message);
                Err(RStatus::Exception)
            }
        }
    })
}

/// Creates a native user-defined class with a supplied namespace.
///
/// The current object-model gate accepts no bases or an explicit `object` base.
/// Later inheritance lowering extends the same ABI shape with C3 MRO creation.
///
/// # Safety
/// Name and base storage must be readable for the duration of the call;
/// `namespace` and `output` must be readable and writable respectively.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_class_new(
    context: *mut RimeraContext,
    name: *const u8,
    name_len: usize,
    bases: *const RValue,
    base_count: usize,
    namespace: *const RValue,
    output: *mut RValue,
) -> RStatus {
    if (bases.is_null() && base_count != 0) || namespace.is_null() || output.is_null() {
        return RStatus::InvalidArgument;
    }
    let Ok(name) = (unsafe { utf8(name, name_len) }) else {
        return RStatus::InvalidArgument;
    };
    let bases = if base_count == 0 {
        &[]
    } else {
        // SAFETY: null was rejected and the caller supplies `base_count` values.
        unsafe { std::slice::from_raw_parts(bases, base_count) }
    };
    // SAFETY: null was rejected and the caller owns one readable namespace value.
    let namespace = unsafe { *namespace };
    with_output(context, output, |context| {
        context.initialize_kernel()?;
        let mut roots = bases.to_vec();
        roots.push(namespace);
        context.with_temporary_roots(&roots, |context| context.new_class(name, bases, namespace))
    })
}

/// Creates an insertion-ordered set, retaining the first occurrence of each value.
///
/// # Safety
/// `values` describes `len` readable values; `output` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_set_new(
    context: *mut RimeraContext,
    values: *const RValue,
    len: usize,
    output: *mut RValue,
) -> RStatus {
    if values.is_null() && len != 0 {
        return RStatus::InvalidArgument;
    }
    let values = if len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(values, len) }
    };
    with_output(context, output, |context| operations::set(context, values))
}

/// Inserts one value in the exact set owned by a comprehension activation.
///
/// # Safety
/// `set` and `value` must be readable values owned by `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_set_insert(
    context: *mut RimeraContext,
    set: *const RValue,
    value: *const RValue,
) -> RStatus {
    if set.is_null() || value.is_null() {
        return RStatus::InvalidArgument;
    }
    let (set, value) = unsafe { (*set, *value) };
    protect(context, |context| {
        match operations::set_add(context, set, value) {
            Ok(()) => Ok(()),
            Err(message) => {
                if context.raised.is_some() {
                    return Err(RStatus::Exception);
                }
                record_exception(context, "TypeError", message);
                Err(RStatus::Exception)
            }
        }
    })
}

/// Unpacks a supported iterable into a managed ABI result array.
///
/// # Safety
/// `value` must be readable and `output` writable for one `RValue`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_unpack(
    context: *mut RimeraContext,
    value: *const RValue,
    fixed_count: usize,
    starred: u8,
    output: *mut RValue,
) -> RStatus {
    if value.is_null() || starred > 1 {
        return RStatus::InvalidArgument;
    }
    let value = unsafe { *value };
    with_output(context, output, |context| {
        operations::unpack(context, value, fixed_count, 0, starred != 0)
    })
}

/// Unpacks an iterable for an exact or extended assignment target.
///
/// `before_count` and `after_count` are the fixed targets on either side of
/// the optional starred target. The result array is returned in source-target
/// order, with the starred slot containing a newly allocated native list.
///
/// # Safety
/// `value` must be readable and `output` writable for one `RValue`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_unpack_ex(
    context: *mut RimeraContext,
    value: *const RValue,
    before_count: usize,
    after_count: usize,
    starred: u8,
    output: *mut RValue,
) -> RStatus {
    if value.is_null() || starred > 1 || (starred == 0 && after_count != 0) {
        return RStatus::InvalidArgument;
    }
    let value = unsafe { *value };
    with_output(context, output, |context| {
        operations::unpack(context, value, before_count, after_count, starred != 0)
    })
}

/// Creates a native range value from integer start, stop, and step values.
///
/// # Safety
/// Inputs must be readable values owned by `context`; `output` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_pattern_sequence(
    context: *mut RimeraContext,
    value: *const RValue,
    before_count: usize,
    after_count: usize,
    starred: u8,
    output: *mut RValue,
    matched: *mut u8,
) -> RStatus {
    if value.is_null() || starred > 1 || (starred == 0 && after_count != 0) {
        return RStatus::InvalidArgument;
    }
    let value = unsafe { *value };
    with_pattern_output(context, output, matched, |context| {
        operations::pattern_sequence_extract(
            context,
            value,
            before_count,
            after_count,
            starred != 0,
        )
    })
}

/// Performs the mapping-pattern type/length preflight before source key
/// expressions are evaluated.
///
/// # Safety
/// `mapping` must be readable and `output` writable for one `RValue`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_pattern_mapping_check(
    context: *mut RimeraContext,
    mapping: *const RValue,
    minimum_count: usize,
    output: *mut RValue,
) -> RStatus {
    if mapping.is_null() {
        return RStatus::InvalidArgument;
    }
    let mapping = unsafe { *mapping };
    with_output(context, output, |context| {
        call::pattern_mapping_check(context, mapping, minimum_count)
    })
}

/// Extracts mapping-pattern values in source-key order and optionally appends
/// a freshly allocated `**rest` dictionary. Missing keys are a normal mismatch.
///
/// # Safety
/// `mapping`, `output`, and `matched` must be valid pointers. `keys` may be null
/// only when `key_count == 0`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_pattern_mapping(
    context: *mut RimeraContext,
    mapping: *const RValue,
    keys: *const RValue,
    key_count: usize,
    include_rest: u8,
    output: *mut RValue,
    matched: *mut u8,
) -> RStatus {
    if mapping.is_null() || include_rest > 1 || (keys.is_null() && key_count != 0) {
        return RStatus::InvalidArgument;
    }
    let mapping = unsafe { *mapping };
    let keys = if key_count == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(keys, key_count) }
    };
    with_pattern_output(context, output, matched, |context| {
        call::pattern_mapping_extract(context, mapping, keys, include_rest != 0)
    })
}

/// Extracts positional and keyword class-pattern attributes. Keyword names are
/// encoded as an internal NUL-separated UTF-8 blob by Cranelift metadata.
///
/// # Safety
/// Value/output pointers must be valid. `keyword_blob` may be null only for an
/// empty blob.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_pattern_class(
    context: *mut RimeraContext,
    subject: *const RValue,
    class: *const RValue,
    positional_count: usize,
    keyword_blob: *const u8,
    keyword_blob_len: usize,
    output: *mut RValue,
    matched: *mut u8,
) -> RStatus {
    if subject.is_null() || class.is_null() || (keyword_blob.is_null() && keyword_blob_len != 0) {
        return RStatus::InvalidArgument;
    }
    let subject = unsafe { *subject };
    let class = unsafe { *class };
    let keyword_names = if keyword_blob_len == 0 {
        Vec::new()
    } else {
        let Ok(blob) = (unsafe { utf8(keyword_blob, keyword_blob_len) }) else {
            return RStatus::InvalidArgument;
        };
        if blob.split('\0').any(str::is_empty) {
            return RStatus::InvalidArgument;
        }
        blob.split('\0').map(str::to_owned).collect::<Vec<_>>()
    };
    with_pattern_output(context, output, matched, |context| {
        call::pattern_class_extract(context, subject, class, positional_count, &keyword_names)
    })
}

/// Creates a native range value from integer start, stop, and step values.
///
/// # Safety
/// Inputs must be readable values owned by `context`; `output` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_range_new(
    context: *mut RimeraContext,
    start: *const RValue,
    stop: *const RValue,
    step: *const RValue,
    output: *mut RValue,
) -> RStatus {
    if start.is_null() || stop.is_null() || step.is_null() {
        return RStatus::InvalidArgument;
    }
    let (start, stop, step) = unsafe { (*start, *stop, *step) };
    with_output(context, output, |context| {
        operations::range(context, start, stop, step)
    })
}

/// Resolves one Python await operand to the iterator driven by the coroutine
/// suspension machinery. Native Rimera coroutines remain on the direct path;
/// arbitrary objects invoke `__await__` exactly once and must return an iterator.
///
/// # Safety
/// `value` must be readable and `output` must be writable for one `RValue`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_await_iterator(
    context: *mut RimeraContext,
    value: *const RValue,
    output: *mut RValue,
) -> RStatus {
    if value.is_null() {
        return RStatus::InvalidArgument;
    }
    let value = unsafe { *value };
    with_output(context, output, |context| {
        operations::await_iterator(context, value)
    })
}

/// Resolves the `async for` iterable through `__aiter__` and validates that the
/// result implements `__anext__`. The returned object remains managed/rootable;
/// no backend or scheduler state is introduced here.
///
/// # Safety
/// `value` must be readable and `output` writable for one `RValue`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_async_iterator_new(
    context: *mut RimeraContext,
    value: *const RValue,
    output: *mut RValue,
) -> RStatus {
    if value.is_null() {
        return RStatus::InvalidArgument;
    }
    let value = unsafe { *value };
    with_output(context, output, |context| {
        operations::async_iterator_new(context, value)
    })
}

/// Calls `__anext__` once for `async for` and returns the awaitable that must be
/// driven by the ordinary coroutine await machinery.
///
/// # Safety
/// `iterator` must be readable and `output` writable for one `RValue`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_async_iterator_next(
    context: *mut RimeraContext,
    iterator: *const RValue,
    output: *mut RValue,
) -> RStatus {
    if iterator.is_null() {
        return RStatus::InvalidArgument;
    }
    let iterator = unsafe { *iterator };
    with_output(context, output, |context| {
        operations::async_iterator_next_awaitable(context, iterator)
    })
}

/// Creates an iterator for a supported native iterable.
///
/// # Safety
/// `value` must be readable and `output` must be writable for one `RValue`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_iterator_new(
    context: *mut RimeraContext,
    value: *const RValue,
    output: *mut RValue,
) -> RStatus {
    if value.is_null() {
        return RStatus::InvalidArgument;
    }
    let value = unsafe { *value };
    with_output(context, output, |context| {
        operations::iterator_new(context, value)
    })
}

/// Probes whether repeated `call(); close()` lifecycle work may be elided for
/// an exact zero-argument Rimera coroutine function. This is deliberately
/// disabled under an explicit managed-heap limit so low-heap allocation/failure
/// behavior remains on the ordinary path.
///
/// # Safety
/// `callable` must be readable and `matched` writable for one `RValue`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_coroutine_close_elide_probe(
    context: *mut RimeraContext,
    callable: *const RValue,
    matched: *mut RValue,
) -> RStatus {
    if context.is_null() || callable.is_null() || matched.is_null() {
        return RStatus::InvalidArgument;
    }
    let context = unsafe { &*context };
    let callable = unsafe { *callable };
    unsafe {
        matched.write(RValue::boolean(call::coroutine_close_elide_probe(
            context, callable,
        )));
    }
    RStatus::Ok
}

/// Probes an exact managed range for the Slice 13 repeat-pure loop collapse.
/// This path is allocation-free and never mutates Python exception state. A
/// false `matched` value means generated code must use the ordinary iterator.
///
/// # Safety
/// `value` must be readable and every output pointer must be writable for one
/// `RValue`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_range_collapse_probe(
    context: *mut RimeraContext,
    value: *const RValue,
    first: *mut RValue,
    last: *mut RValue,
    matched: *mut RValue,
    nonempty: *mut RValue,
) -> RStatus {
    if context.is_null()
        || value.is_null()
        || first.is_null()
        || last.is_null()
        || matched.is_null()
        || nonempty.is_null()
    {
        return RStatus::InvalidArgument;
    }
    let context = unsafe { &*context };
    let value = unsafe { *value };
    let probe = call::range_collapse_probe(context, value);
    unsafe {
        if let Some((first_value, last_value, has_values)) = probe {
            first.write(first_value);
            last.write(last_value);
            matched.write(RValue::boolean(true));
            nonempty.write(RValue::boolean(has_values));
        } else {
            first.write(RValue::NONE);
            last.write(RValue::NONE);
            matched.write(RValue::boolean(false));
            nonempty.write(RValue::boolean(false));
        }
    }
    RStatus::Ok
}

/// Advances an iterator without using Python exception state for exhaustion.
///
/// # Safety
/// `iterator`, `output`, and `has_value` must be readable/writable storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_iterator_next(
    context: *mut RimeraContext,
    iterator: *const RValue,
    output: *mut RValue,
    has_value: *mut u8,
) -> RStatus {
    if iterator.is_null() || output.is_null() || has_value.is_null() {
        return RStatus::InvalidArgument;
    }
    let iterator = unsafe { *iterator };
    protect(context, |context| {
        match operations::iterator_next(context, iterator) {
            Ok(Some(value)) => {
                unsafe {
                    output.write(value);
                    has_value.write(1);
                }
                Ok(())
            }
            Ok(None) => {
                unsafe {
                    has_value.write(0);
                }
                Ok(())
            }
            Err(_) if context.raised.is_some() => Err(RStatus::Exception),
            Err(message) => {
                record_exception(context, "TypeError", message);
                Err(RStatus::Exception)
            }
        }
    })
}

/// Returns the number of elements in one managed value.
///
/// # Safety
/// `value` and `output` must be readable and writable respectively.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_length(
    context: *mut RimeraContext,
    value: *const RValue,
    output: *mut RValue,
) -> RStatus {
    if value.is_null() {
        return RStatus::InvalidArgument;
    }
    let value = unsafe { *value };
    with_output(context, output, |context| {
        operations::length(context, value)
    })
}

/// Reads one integer-indexed list or tuple element.
///
/// # Safety
/// Inputs must be readable and `output` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_item_get(
    context: *mut RimeraContext,
    collection: *const RValue,
    index: *const RValue,
    output: *mut RValue,
) -> RStatus {
    if collection.is_null() || index.is_null() {
        return RStatus::InvalidArgument;
    }
    let collection = unsafe { *collection };
    let index = unsafe { *index };
    with_output(context, output, |context| {
        operations::item_get(context, collection, index)
    })
}

/// Stores one integer-indexed list element.
///
/// # Safety
/// `collection`, `index`, and `value` must be readable values owned by
/// `context`, which must be a live context returned by this ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_item_set(
    context: *mut RimeraContext,
    collection: *const RValue,
    index: *const RValue,
    value: *const RValue,
) -> RStatus {
    if collection.is_null() || index.is_null() || value.is_null() {
        return RStatus::InvalidArgument;
    }
    let (collection, index, value) = unsafe { (*collection, *index, *value) };
    protect(context, |context| {
        match operations::item_set(context, collection, index, value) {
            Ok(()) => Ok(()),
            Err(message) => {
                if context.raised.is_some() {
                    return Err(RStatus::Exception);
                }
                let exception_type = if message == "managed heap limit exceeded" {
                    "MemoryError"
                } else {
                    "TypeError"
                };
                record_exception(context, exception_type, message);
                Err(RStatus::Exception)
            }
        }
    })
}

/// Deletes an item through the native item protocol.
///
/// # Safety
/// Inputs must reference live values in this context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_item_delete(
    context: *mut RimeraContext,
    collection: *const RValue,
    index: *const RValue,
) -> RStatus {
    if collection.is_null() || index.is_null() {
        return RStatus::InvalidArgument;
    }
    let (collection, index) = unsafe { (*collection, *index) };
    protect(context, |context| {
        operations::item_delete(context, collection, index).map_err(|message| {
            if context.raised.is_none() {
                record_exception(context, "TypeError", message);
            }
            RStatus::Exception
        })
    })
}

/// Tests membership in a supported native container.
///
/// # Safety
/// Inputs must be readable and `output` writable for one `RValue`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_contains(
    context: *mut RimeraContext,
    collection: *const RValue,
    needle: *const RValue,
    output: *mut RValue,
) -> RStatus {
    if collection.is_null() || needle.is_null() {
        return RStatus::InvalidArgument;
    }
    let (collection, needle) = unsafe { (*collection, *needle) };
    with_output(context, output, |context| {
        operations::contains(context, collection, needle).map(RValue::boolean)
    })
}

/// Reads one value from an internal managed value array.
///
/// # Safety
/// `array` must reference a live value and `output` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_value_array_get(
    context: *mut RimeraContext,
    array: *const RValue,
    index: usize,
    output: *mut RValue,
) -> RStatus {
    if array.is_null() {
        return RStatus::InvalidArgument;
    }
    // SAFETY: null was rejected and the input is read-only for this call.
    let array = unsafe { *array };
    with_output(context, output, |context| {
        operations::value_array_get(context, array, index)
    })
}

/// Applies a native unary operation.
///
/// # Safety
/// Input and output pointers must reference live values in this context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_unary(
    context: *mut RimeraContext,
    op: u8,
    operand: *const RValue,
    output: *mut RValue,
) -> RStatus {
    if operand.is_null() {
        return RStatus::InvalidArgument;
    }
    // SAFETY: null was rejected and input is read-only for this call.
    let operand = unsafe { *operand };
    with_output(context, output, |context| {
        operations::unary(context, op, operand)
    })
}

/// Applies a native binary operation.
///
/// # Safety
/// Input and output pointers must reference live values in this context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_binary(
    context: *mut RimeraContext,
    op: u8,
    left: *const RValue,
    right: *const RValue,
    output: *mut RValue,
) -> RStatus {
    if left.is_null() || right.is_null() {
        return RStatus::InvalidArgument;
    }
    // SAFETY: null was rejected and inputs are read-only for this call.
    let (left, right) = unsafe { (*left, *right) };
    with_output(context, output, |context| {
        operations::binary(context, op, left, right)
    })
}

/// Applies a native in-place binary operation.
///
/// # Safety
/// Inputs and output pointers must reference live values in this context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_inplace(
    context: *mut RimeraContext,
    op: u8,
    left: *const RValue,
    right: *const RValue,
    output: *mut RValue,
) -> RStatus {
    if left.is_null() || right.is_null() {
        return RStatus::InvalidArgument;
    }
    let (left, right) = unsafe { (*left, *right) };
    with_output(context, output, |context| {
        operations::inplace(context, op, left, right)
    })
}

/// Compares two native values.
///
/// # Safety
/// Input and output pointers must reference live values in this context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_compare(
    context: *mut RimeraContext,
    op: u8,
    left: *const RValue,
    right: *const RValue,
    output: *mut RValue,
) -> RStatus {
    if left.is_null() || right.is_null() {
        return RStatus::InvalidArgument;
    }
    // SAFETY: null was rejected and inputs are read-only for this call.
    let (left, right) = unsafe { (*left, *right) };
    with_output(context, output, |context| {
        operations::compare(context, op, left, right)
    })
}

/// Applies an f-string conversion and then the ordinary formatting protocol.
///
/// # Safety
/// `value`, `spec`, and `output` must reference live values in this context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_format_value(
    context: *mut RimeraContext,
    conversion: u8,
    value: *const RValue,
    spec: *const RValue,
    output: *mut RValue,
) -> RStatus {
    if value.is_null() || spec.is_null() {
        return RStatus::InvalidArgument;
    }
    let Ok(conversion) = RFormatConversion::try_from(conversion) else {
        return RStatus::InvalidArgument;
    };
    // SAFETY: null was rejected and inputs are read-only for this call.
    let (value, spec) = unsafe { (*value, *spec) };
    with_output(context, output, |context| {
        operations::format_value(context, value, conversion, spec)
    })
}

/// Computes Python truthiness for a native value.
///
/// # Safety
/// Input and output pointers must reference live values in this context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_truthy(
    context: *mut RimeraContext,
    value: *const RValue,
    output: *mut RValue,
) -> RStatus {
    if value.is_null() {
        return RStatus::InvalidArgument;
    }
    // SAFETY: null was rejected and input is read-only for this call.
    let value = unsafe { *value };
    with_output(context, output, |context| {
        operations::truthy(context, value).map(RValue::boolean)
    })
}

/// Writes values with Python's default `print` formatting.
///
/// # Safety
/// `values` must describe `len` readable values owned by this context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_print(
    context: *mut RimeraContext,
    values: *const RValue,
    len: usize,
) -> RStatus {
    if values.is_null() && len != 0 {
        return RStatus::InvalidArgument;
    }
    let values = if len == 0 {
        &[]
    } else {
        // SAFETY: non-null was checked and the caller supplies `len` values.
        unsafe { std::slice::from_raw_parts(values, len) }
    };
    protect(context, |context| {
        let rendered = values
            .iter()
            .map(|value| operations::stringify(context, *value))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|message| {
                context.fail(message);
                RStatus::Exception
            })?;
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "{}", rendered.join(" ")).map_err(|_| RStatus::Exception)?;
        Ok(())
    })
}

/// Displays one interactive expression using repr; None has no output.
///
/// # Safety
/// `value` points to a live value owned by `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_display(
    context: *mut RimeraContext,
    value: *const RValue,
) -> RStatus {
    if value.is_null() {
        return RStatus::InvalidArgument;
    }
    let value = unsafe { *value };
    if value == RValue::NONE {
        return RStatus::Ok;
    }
    protect(context, |context| {
        context
            .with_temporary_roots(&[value], |context| {
                let rendered = operations::repr(context, value)?;
                let text = operations::string_value(context, rendered)
                    .ok_or("repr must return a string")?;
                writeln!(io::stdout().lock(), "{text}").map_err(|error| error.to_string())?;
                let builtins = context.builtins().ok_or("builtins unavailable")?;
                context.namespace_set(builtins, "_", value)
            })
            .map_err(|message| {
                record_exception(context, "RuntimeError", message);
                RStatus::Exception
            })
    })
}

/// Writes a compile-time UTF-8 string literal followed by a newline.
///
/// This is the small release path for a proven builtin `print("literal")` call.
/// It bypasses generic object formatting because semantic analysis has already
/// proven that the argument is exactly a Python `str` literal.
///
/// # Safety
/// `bytes` must describe `len` readable UTF-8 bytes for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_print_literal(
    context: *mut RimeraContext,
    bytes: *const u8,
    len: usize,
) -> RStatus {
    if context.is_null() || (bytes.is_null() && len != 0) {
        return RStatus::InvalidArgument;
    }

    fn write_all(mut bytes: &[u8]) -> bool {
        while !bytes.is_empty() {
            // SAFETY: `bytes` is live for this call and stdout is file descriptor 1.
            let written = unsafe { c_write(1, bytes.as_ptr().cast::<c_void>(), bytes.len()) };
            if written <= 0 {
                return false;
            }
            bytes = &bytes[written as usize..];
        }
        true
    }

    let bytes = if len == 0 {
        &[][..]
    } else {
        // SAFETY: non-null was checked and the caller supplies `len` readable bytes.
        unsafe { std::slice::from_raw_parts(bytes, len) }
    };
    if write_all(bytes) && write_all(b"\n") {
        RStatus::Ok
    } else {
        // SAFETY: null was rejected and callers own a live context returned by this ABI.
        unsafe { &mut *context }.fail("failed to write stdout");
        RStatus::Exception
    }
}

/// Forces a tracing collection.
///
/// # Safety
/// `context` must be a live context returned by this ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_collect(context: *mut RimeraContext) -> RStatus {
    protect(context, |context| {
        context.collect();
        Ok(())
    })
}

/// Renders and clears the context's active runtime failure.
///
/// # Safety
/// `context` must be null or a live context returned by this ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_render_error(context: *mut RimeraContext) {
    if context.is_null() {
        eprintln!("Rimera runtime error: invalid context");
        return;
    }
    // SAFETY: null was rejected and context is live for this call.
    let context = unsafe { &mut *context };
    let rendered = context.render_active_exception();
    let color = runtime_color_enabled();
    let rendered = format_runtime_error(&rendered, color);
    let mut stderr = io::stderr().lock();
    let _ = writeln!(stderr, "{rendered}");
}

fn runtime_color_enabled() -> bool {
    match std::env::var("RIMERA_COLOR").as_deref() {
        Ok("always") => true,
        Ok("never") => false,
        Ok("auto") | Err(std::env::VarError::NotPresent) => {
            io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none()
        }
        Ok(_) | Err(std::env::VarError::NotUnicode(_)) => {
            io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none()
        }
    }
}

/// Applies terminal presentation without changing the runtime's canonical
/// traceback text. Captured output deliberately remains plain for tooling.
fn format_runtime_error(rendered: &str, color: bool) -> String {
    if !color {
        return rendered.to_owned();
    }

    rendered
        .lines()
        .map(|line| {
            if line == "Traceback (most recent call last):" || is_exception_summary(line) {
                format!("\x1b[1;31m{line}\x1b[0m")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_exception_summary(line: &str) -> bool {
    !line.chars().next().is_some_and(char::is_whitespace)
        && !line.starts_with("Traceback ")
        && !line.starts_with("During handling ")
        && !line.starts_with("The above exception ")
        && line
            .split_once(':')
            .is_some_and(|(name, _)| name.ends_with("Error") || name.ends_with("Exception"))
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering as AtomicOrdering},
    };

    use std::task::{Context, Poll, Waker};

    use super::*;
    use crate::heap::HeapObject;
    use crate::object::{
        ExceptionObject, FunctionKind, FunctionObject, ModuleObject, ModuleState, Parameter,
        TracebackObject, ValueDictionaryObject,
    };
    use crate::{AsyncRuntimeDriver, ParameterKind};
    use rimera_abi::{RCompareOperator, RGeneratorOperation, RGeneratorOutcome};

    unsafe extern "C" fn two_stage_generator_resume(
        context: *mut c_void,
        generator: *const RValue,
        _operation: RGeneratorOperation,
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
        // SAFETY: the runtime validated every pointer before invoking this
        // test-only generated-resume entry.
        let context = unsafe { &mut *context.cast::<RimeraContext>() };
        let generator = unsafe { *generator };
        let input = unsafe { *input };
        let Some(HeapObject::Generator(object)) = context.heap.get_mut(generator) else {
            return RStatus::InvalidArgument;
        };
        match object.state {
            0 => {
                object.state = 1;
                unsafe {
                    output.write(RValue::small_int(7));
                    outcome.write(RGeneratorOutcome::Yielded);
                }
            }
            1 => {
                object.state = 2;
                unsafe {
                    output.write(input);
                    outcome.write(RGeneratorOutcome::Returned);
                }
            }
            _ => return RStatus::InvalidArgument,
        }
        RStatus::Ok
    }

    static GENERATOR_CLOSE_COUNT: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "C" fn finalizable_generator_resume(
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
        let Some(HeapObject::Generator(object)) = context.heap.get_mut(generator) else {
            return RStatus::InvalidArgument;
        };
        if matches!(operation, RGeneratorOperation::Close) {
            GENERATOR_CLOSE_COUNT.fetch_add(1, AtomicOrdering::SeqCst);
        }
        if object.state == 0 {
            object.state = 1;
            unsafe {
                output.write(RValue::small_int(1));
                outcome.write(RGeneratorOutcome::Yielded);
            }
        } else {
            unsafe {
                output.write(RValue::NONE);
                outcome.write(RGeneratorOutcome::Returned);
            }
        }
        RStatus::Ok
    }

    unsafe extern "C" fn add_native(
        context: *mut c_void,
        _function: *const RValue,
        bound: *const RValue,
        bound_len: usize,
        output: *mut RValue,
    ) -> RStatus {
        if context.is_null() || bound.is_null() || bound_len != 5 || output.is_null() {
            return RStatus::InvalidArgument;
        }
        let context = unsafe { &mut *context.cast::<RimeraContext>() };
        let bound = unsafe { std::slice::from_raw_parts(bound, bound_len) };
        if !matches!(context.heap.get(bound[2]), Some(HeapObject::Tuple(_)))
            || !matches!(context.heap.get(bound[4]), Some(HeapObject::Dictionary(_)))
        {
            return RStatus::InvalidArgument;
        }
        match operations::binary(context, 0, bound[0], bound[1]) {
            Ok(value) => {
                unsafe { output.write(value) };
                RStatus::Ok
            }
            Err(message) => {
                context.fail(message);
                RStatus::Exception
            }
        }
    }

    fn test_function(
        context: &mut RimeraContext,
        code_address: usize,
        kind: FunctionKind,
        name: &str,
        qualified_name: &str,
        parameters: Vec<Parameter>,
        closure: &[RValue],
    ) -> RValue {
        let positional_arity = parameters
            .iter()
            .all(|parameter| {
                matches!(
                    parameter.kind,
                    ParameterKind::PositionalOnly | ParameterKind::PositionalOrKeyword
                )
            })
            .then_some(parameters.len());
        let roots = closure.to_vec();
        context
            .with_temporary_roots(&roots, |context| {
                let closure_value = if closure.is_empty() {
                    None
                } else {
                    Some(operations::tuple(context, closure)?)
                };
                let mut metadata_roots = roots.clone();
                closure_value
                    .into_iter()
                    .for_each(|value| metadata_roots.push(value));
                let free_names = (0..closure.len())
                    .map(|index| format!("free_{index}"))
                    .collect::<Vec<_>>();
                let code = context.with_temporary_roots(&metadata_roots, |context| {
                    context.allocate(HeapObject::Code(CodeObject {
                        dynamic_mode: None,
                        flags_override: None,
                        native_unit_address: None,
                        code_address,
                        kind,
                        name: name.to_owned(),
                        qualified_name: qualified_name.to_owned(),
                        parameters: parameters.into_boxed_slice(),
                        filename: "<runtime-test>".to_owned(),
                        first_line: 1,
                        local_names: Box::new([]),
                        cell_names: Box::new([]),
                        free_names: free_names.into_boxed_slice(),
                    }))
                })?;
                metadata_roots.push(code);
                let globals = context.globals().unwrap();
                metadata_roots.push(globals);
                context.with_temporary_roots(&metadata_roots, |context| {
                    let function = context.allocate(HeapObject::Function(FunctionObject {
                        code,
                        fast_call: FastCallMetadata::new(code_address, 1, positional_arity, kind),
                        globals,
                        name: name.to_owned(),
                        qualified_name: qualified_name.to_owned(),
                        closure: closure_value,
                        defaults: None,
                        keyword_defaults: None,
                        annotations: None,
                        type_params: None,
                    }))?;
                    context.capture_function_builtins(function, globals);
                    Ok::<RValue, String>(function)
                })
            })
            .unwrap()
    }

    static BUFFER_TEST_LOCK: Mutex<()> = Mutex::new(());
    static BUFFER_ACQUIRE_COUNT: AtomicUsize = AtomicUsize::new(0);
    static BUFFER_RELEASE_COUNT: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "C" fn buffer_provider_native(
        context: *mut c_void,
        _function: *const RValue,
        bound: *const RValue,
        bound_len: usize,
        output: *mut RValue,
    ) -> RStatus {
        if context.is_null() || bound.is_null() || bound_len != 2 || output.is_null() {
            return RStatus::InvalidArgument;
        }
        let context = unsafe { &mut *context.cast::<RimeraContext>() };
        let bound = unsafe { std::slice::from_raw_parts(bound, bound_len) };
        if bound[1] != RValue::small_int(284) {
            return RStatus::InvalidArgument;
        }
        BUFFER_ACQUIRE_COUNT.fetch_add(1, AtomicOrdering::SeqCst);
        let view = match context.attribute_get(bound[0], "inner") {
            Ok(view) if matches!(context.heap.get(view), Some(HeapObject::MemoryView(_))) => view,
            Ok(_) => return RStatus::InvalidArgument,
            Err(message) => {
                context.fail(message);
                return RStatus::Exception;
            }
        };
        unsafe { output.write(view) };
        RStatus::Ok
    }

    unsafe extern "C" fn constrained_buffer_provider_native(
        context: *mut c_void,
        function: *const RValue,
        bound: *const RValue,
        bound_len: usize,
        output: *mut RValue,
    ) -> RStatus {
        let status = unsafe { buffer_provider_native(context, function, bound, bound_len, output) };
        if status != RStatus::Ok {
            return status;
        }
        let context = unsafe { &mut *context.cast::<RimeraContext>() };
        let limit = context.stats().live_bytes;
        match context.set_heap_limit(Some(limit)) {
            Ok(()) => RStatus::Ok,
            Err(message) => {
                context.fail(message);
                RStatus::Exception
            }
        }
    }

    unsafe extern "C" fn counting_buffer_release_native(
        context: *mut c_void,
        _function: *const RValue,
        bound: *const RValue,
        bound_len: usize,
        output: *mut RValue,
    ) -> RStatus {
        if context.is_null() || bound.is_null() || bound_len != 2 || output.is_null() {
            return RStatus::InvalidArgument;
        }
        let context = unsafe { &mut *context.cast::<RimeraContext>() };
        let bound = unsafe { std::slice::from_raw_parts(bound, bound_len) };
        if !matches!(context.heap.get(bound[1]), Some(HeapObject::MemoryView(_))) {
            return RStatus::InvalidArgument;
        }
        BUFFER_RELEASE_COUNT.fetch_add(1, AtomicOrdering::SeqCst);
        unsafe { output.write(RValue::NONE) };
        RStatus::Ok
    }

    unsafe extern "C" fn raising_buffer_release_native(
        context: *mut c_void,
        function: *const RValue,
        bound: *const RValue,
        bound_len: usize,
        output: *mut RValue,
    ) -> RStatus {
        let status =
            unsafe { counting_buffer_release_native(context, function, bound, bound_len, output) };
        if status != RStatus::Ok {
            return status;
        }
        let context = unsafe { &mut *context.cast::<RimeraContext>() };
        match context.raise_builtin("RuntimeError", "release boom") {
            Ok(_) => RStatus::Exception,
            Err(message) => {
                context.fail(message);
                RStatus::Exception
            }
        }
    }

    fn make_buffer_provider(
        context: &mut RimeraContext,
        buffer_code: usize,
        release_code: usize,
    ) -> (RValue, RValue, RValue) {
        let namespace = context.namespace_new().unwrap();
        let buffer = test_function(
            context,
            buffer_code,
            FunctionKind::Normal,
            "__buffer__",
            "Provider.__buffer__",
            vec![
                Parameter {
                    name: "self".to_owned(),
                    kind: ParameterKind::PositionalOrKeyword,
                    has_default: false,
                },
                Parameter {
                    name: "flags".to_owned(),
                    kind: ParameterKind::PositionalOrKeyword,
                    has_default: false,
                },
            ],
            &[],
        );
        let release = context.with_temporary_roots(&[namespace, buffer], |context| {
            test_function(
                context,
                release_code,
                FunctionKind::Normal,
                "__release_buffer__",
                "Provider.__release_buffer__",
                vec![
                    Parameter {
                        name: "self".to_owned(),
                        kind: ParameterKind::PositionalOrKeyword,
                        has_default: false,
                    },
                    Parameter {
                        name: "view".to_owned(),
                        kind: ParameterKind::PositionalOrKeyword,
                        has_default: false,
                    },
                ],
                &[],
            )
        });
        context
            .namespace_set(namespace, "__buffer__", buffer)
            .unwrap();
        context
            .namespace_set(namespace, "__release_buffer__", release)
            .unwrap();
        let class = context.new_class("Provider", &[], namespace).unwrap();
        let provider = context.new_instance(class).unwrap();
        let data = operations::bytearray(context, b"abcd").unwrap();
        let inner = context
            .with_temporary_roots(&[provider, data], |context| {
                operations::memoryview(context, data)
            })
            .unwrap();
        context.attribute_set(provider, "data", data).unwrap();
        context.attribute_set(provider, "inner", inner).unwrap();
        (provider, data, inner)
    }

    unsafe extern "C" fn collecting_hash_native(
        context: *mut c_void,
        _function: *const RValue,
        bound: *const RValue,
        bound_len: usize,
        output: *mut RValue,
    ) -> RStatus {
        if context.is_null() || bound.is_null() || bound_len != 1 || output.is_null() {
            return RStatus::InvalidArgument;
        }
        let context = unsafe { &mut *context.cast::<RimeraContext>() };
        context.collect();
        unsafe { output.write(RValue::small_int(17)) };
        RStatus::Ok
    }

    unsafe extern "C" fn collecting_mutating_hash_native(
        context: *mut c_void,
        function: *const RValue,
        bound: *const RValue,
        bound_len: usize,
        output: *mut RValue,
    ) -> RStatus {
        if context.is_null()
            || function.is_null()
            || bound.is_null()
            || bound_len != 1
            || output.is_null()
        {
            return RStatus::InvalidArgument;
        }
        let context = unsafe { &mut *context.cast::<RimeraContext>() };
        let function = unsafe { *function };
        let unrelated = match context.heap.get(function) {
            Some(HeapObject::Function(function)) => {
                function
                    .closure
                    .and_then(|closure| match context.heap.get(closure) {
                        Some(HeapObject::Tuple(cells)) => cells.first().copied(),
                        _ => None,
                    })
            }
            _ => None,
        };
        let Some(unrelated) = unrelated else {
            return RStatus::InvalidArgument;
        };
        context.collect();
        if let Err(message) = operations::item_set(
            context,
            unrelated,
            RValue::small_int(1),
            RValue::small_int(99),
        ) {
            context.fail(message);
            return RStatus::Exception;
        }
        unsafe { output.write(RValue::small_int(17)) };
        RStatus::Ok
    }

    unsafe extern "C" fn collecting_equal_native(
        context: *mut c_void,
        _function: *const RValue,
        bound: *const RValue,
        bound_len: usize,
        output: *mut RValue,
    ) -> RStatus {
        if context.is_null() || bound.is_null() || bound_len != 2 || output.is_null() {
            return RStatus::InvalidArgument;
        }
        let context = unsafe { &mut *context.cast::<RimeraContext>() };
        context.collect();
        unsafe { output.write(RValue::boolean(true)) };
        RStatus::Ok
    }

    unsafe extern "C" fn collecting_truth_native(
        context: *mut c_void,
        _function: *const RValue,
        bound: *const RValue,
        bound_len: usize,
        output: *mut RValue,
    ) -> RStatus {
        if context.is_null() || bound.is_null() || bound_len != 1 || output.is_null() {
            return RStatus::InvalidArgument;
        }
        let context = unsafe { &mut *context.cast::<RimeraContext>() };
        let bound = unsafe { std::slice::from_raw_parts(bound, bound_len) };
        context.collect();
        if !matches!(context.heap.get(bound[0]), Some(HeapObject::Instance(_))) {
            return RStatus::InvalidArgument;
        }
        unsafe { output.write(RValue::boolean(true)) };
        RStatus::Ok
    }

    unsafe extern "C" fn collecting_less_native(
        context: *mut c_void,
        _function: *const RValue,
        bound: *const RValue,
        bound_len: usize,
        output: *mut RValue,
    ) -> RStatus {
        if context.is_null() || bound.is_null() || bound_len != 2 || output.is_null() {
            return RStatus::InvalidArgument;
        }
        let context = unsafe { &mut *context.cast::<RimeraContext>() };
        let bound = unsafe { std::slice::from_raw_parts(bound, bound_len) };
        context.collect();
        if !matches!(context.heap.get(bound[0]), Some(HeapObject::Instance(_)))
            || !matches!(context.heap.get(bound[1]), Some(HeapObject::Instance(_)))
        {
            return RStatus::InvalidArgument;
        }
        unsafe { output.write(RValue::boolean(true)) };
        RStatus::Ok
    }

    unsafe extern "C" fn collecting_setitem_native(
        context: *mut c_void,
        _function: *const RValue,
        bound: *const RValue,
        bound_len: usize,
        output: *mut RValue,
    ) -> RStatus {
        if context.is_null() || bound.is_null() || bound_len != 3 || output.is_null() {
            return RStatus::InvalidArgument;
        }
        let context = unsafe { &mut *context.cast::<RimeraContext>() };
        let bound = unsafe { std::slice::from_raw_parts(bound, bound_len) };
        context.collect();
        if !matches!(context.heap.get(bound[0]), Some(HeapObject::Instance(_))) {
            return RStatus::InvalidArgument;
        }
        let tuple_values = match context.heap.get(bound[1]) {
            Some(HeapObject::Tuple(values)) => values.to_vec(),
            _ => return RStatus::InvalidArgument,
        };
        if tuple_values.len() != 2
            || !matches!(
                context.heap.get(tuple_values[1]),
                Some(HeapObject::Slice(_))
            )
            || operations::display(context, bound[2]).as_deref() != Ok("rhs")
        {
            return RStatus::InvalidArgument;
        }
        unsafe { output.write(RValue::NONE) };
        RStatus::Ok
    }

    #[test]
    fn rooted_values_survive_forced_collection() {
        let mut context = RimeraContext::default();
        let mut value = operations::string(&mut context, "alive").unwrap();
        let mut frame = RRootFrame::new(&raw mut value, 1);
        // SAFETY: context and frame remain live for both calls.
        assert_eq!(
            unsafe { rimera_roots_push(&raw mut context, &raw mut frame) },
            RStatus::Ok
        );
        context.collect();
        assert_eq!(operations::display(&context, value).unwrap(), "alive");
        // SAFETY: this removes the active frame before either value is dropped.
        assert_eq!(
            unsafe { rimera_roots_pop(&raw mut context, &raw mut frame) },
            RStatus::Ok
        );
    }

    #[test]
    fn generator_resume_preserves_persistent_slots_across_collection() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let captured = operations::string(&mut context, "live across suspension").unwrap();
        let function = test_function(
            &mut context,
            two_stage_generator_resume as usize,
            FunctionKind::Generator {
                persistent_slot_count: 1,
            },
            "two_stage",
            "two_stage",
            Vec::new(),
            &[],
        );
        let mut generator = context.new_generator(function, &[captured]).unwrap();
        let mut frame = RRootFrame::new(&raw mut generator, 1);
        // SAFETY: the context and frame remain live for the registered scope.
        assert_eq!(
            unsafe { rimera_roots_push(&raw mut context, &raw mut frame) },
            RStatus::Ok
        );

        context.collect();
        let Some(HeapObject::Generator(object)) = context.heap.get(generator) else {
            panic!("rooted generator was collected");
        };
        assert_eq!(object.slots[0], Some(captured));
        assert_eq!(
            operations::display(&context, captured).unwrap(),
            "live across suspension"
        );

        let first = context
            .resume_generator(generator, RGeneratorOperation::Next, RValue::NONE)
            .unwrap();
        assert_eq!(first.outcome, RGeneratorOutcome::Yielded);
        assert_eq!(first.value, RValue::small_int(7));

        context.collect();
        let final_result = context
            .resume_generator(generator, RGeneratorOperation::Send, RValue::small_int(42))
            .unwrap();
        assert_eq!(final_result.outcome, RGeneratorOutcome::Returned);
        assert_eq!(final_result.value, RValue::small_int(42));
        let Some(HeapObject::Generator(object)) = context.heap.get(generator) else {
            panic!("generator completed unexpectedly");
        };
        assert!(object.completed);
        assert!(object.slots.iter().all(Option::is_none));

        // SAFETY: unregister the stack frame before its backing storage ends.
        assert_eq!(
            unsafe { rimera_roots_pop(&raw mut context, &raw mut frame) },
            RStatus::Ok
        );
    }

    #[test]
    fn unreachable_suspended_generators_close_once_before_collection() {
        GENERATOR_CLOSE_COUNT.store(0, AtomicOrdering::SeqCst);
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let function = test_function(
            &mut context,
            finalizable_generator_resume as usize,
            FunctionKind::Generator {
                persistent_slot_count: 0,
            },
            "finalizable",
            "finalizable",
            Vec::new(),
            &[],
        );
        context.add_context_root(function);
        let generator = context.new_generator(function, &[]).unwrap();
        let first = context
            .resume_generator(generator, RGeneratorOperation::Next, RValue::NONE)
            .unwrap();
        assert_eq!(first.outcome, RGeneratorOutcome::Yielded);

        context.collect();

        assert_eq!(GENERATOR_CLOSE_COUNT.load(AtomicOrdering::SeqCst), 1);
        assert!(context.heap.get(generator).is_none());
    }

    #[test]
    fn generator_delegate_and_pending_exception_graphs_are_traced_and_fail_atomically() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let function = test_function(
            &mut context,
            two_stage_generator_resume as usize,
            FunctionKind::Generator {
                persistent_slot_count: 1,
            },
            "delegating",
            "delegating",
            Vec::new(),
            &[],
        );
        context.add_context_root(function);
        let delegate =
            operations::list(&mut context, &[RValue::small_int(1), RValue::small_int(2)]).unwrap();
        let message = operations::string(&mut context, "pending").unwrap();
        let pending = context
            .new_builtin_exception("ValueError", &[message])
            .unwrap();
        let mut generator = context.new_generator(function, &[]).unwrap();
        let Some(HeapObject::Generator(object)) = context.heap.get_mut(generator) else {
            panic!("expected managed generator");
        };
        object.delegate = Some(delegate);
        object.raised = Some(pending);
        object.handled = vec![pending].into_boxed_slice();
        let mut frame = RRootFrame::new(&raw mut generator, 1);
        assert_eq!(
            unsafe { rimera_roots_push(&raw mut context, &raw mut frame) },
            RStatus::Ok
        );
        context.collect();
        assert!(context.heap.get(generator).is_some());
        assert!(context.heap.get(delegate).is_some());
        assert!(context.heap.get(pending).is_some());
        assert_eq!(
            unsafe { rimera_roots_pop(&raw mut context, &raw mut frame) },
            RStatus::Ok
        );
        context.collect();
        assert!(context.heap.get(generator).is_none());
        assert!(context.heap.get(delegate).is_none());
        assert!(context.heap.get(pending).is_none());

        let live_before = context.stats().live;
        let live_bytes = context.stats().live_bytes;
        context.set_heap_limit(Some(live_bytes)).unwrap();
        assert_eq!(
            context.new_generator(function, &[RValue::small_int(1)]),
            Err("managed heap limit exceeded".to_owned())
        );
        assert_eq!(context.stats().live, live_before);
        context.set_heap_limit(None).unwrap();
    }

    #[test]
    fn iter_and_next_use_the_generic_builtin_call_path() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let iter = context.ensure_builtin("iter").unwrap().unwrap();
        let next = context.ensure_builtin("next").unwrap().unwrap();
        let range = operations::range(
            &mut context,
            RValue::small_int(0),
            RValue::small_int(2),
            RValue::small_int(1),
        )
        .unwrap();
        let iterator = call::invoke(&mut context, iter, &[range], &[]).unwrap();
        assert_eq!(
            call::invoke(&mut context, next, &[iterator], &[]).unwrap(),
            RValue::small_int(0)
        );
        assert_eq!(
            call::invoke(&mut context, next, &[iterator], &[]).unwrap(),
            RValue::small_int(1)
        );
        assert!(call::invoke(&mut context, next, &[iterator], &[]).is_err());
        let raised = context
            .raised
            .expect("next exhaustion records StopIteration");
        assert_eq!(context.exception_type_name(raised), Some("StopIteration"));
    }

    #[test]
    fn range_slicing_uses_arbitrary_precision_native_bounds() {
        let mut context = RimeraContext::default();
        let range = operations::range(
            &mut context,
            RValue::small_int(0),
            RValue::small_int(10),
            RValue::small_int(2),
        )
        .unwrap();
        let slice = operations::slice(
            &mut context,
            Some(RValue::small_int(1)),
            Some(RValue::small_int(4)),
            Some(RValue::small_int(2)),
        )
        .unwrap();
        let sliced = operations::item_get(&mut context, range, slice).unwrap();
        assert_eq!(
            operations::display(&context, sliced).unwrap(),
            "range(2, 8, 4)"
        );
    }

    #[test]
    fn rooted_list_traces_its_managed_elements() {
        let mut context = RimeraContext::default();
        let child = operations::string(&mut context, "alive child").unwrap();
        let mut list = operations::list(&mut context, &[child]).unwrap();
        let mut frame = RRootFrame::new(&raw mut list, 1);
        // SAFETY: context and the list root remain live through collection.
        assert_eq!(
            unsafe { rimera_roots_push(&raw mut context, &raw mut frame) },
            RStatus::Ok
        );
        context.collect();
        assert_eq!(
            operations::display(&context, list).unwrap(),
            "['alive child']"
        );
        // SAFETY: removes the active frame before its storage is dropped.
        assert_eq!(
            unsafe { rimera_roots_pop(&raw mut context, &raw mut frame) },
            RStatus::Ok
        );
    }

    #[test]
    fn rooted_values_survive_repeated_collections() {
        let mut context = RimeraContext::default();
        let mut values = [
            operations::string(&mut context, "first").unwrap(),
            operations::int_from_decimal(&mut context, "999999999999999999999999999999").unwrap(),
        ];
        let mut frame = RRootFrame::new(values.as_mut_ptr(), values.len());
        // SAFETY: context, frame, and slots remain live for the registration.
        assert_eq!(
            unsafe { rimera_roots_push(&raw mut context, &raw mut frame) },
            RStatus::Ok
        );
        for _ in 0..8 {
            context.collect();
        }
        assert_eq!(operations::display(&context, values[0]).unwrap(), "first");
        assert_eq!(
            operations::display(&context, values[1]).unwrap(),
            "999999999999999999999999999999"
        );
        // SAFETY: removes the active root frame before its backing slots drop.
        assert_eq!(
            unsafe { rimera_roots_pop(&raw mut context, &raw mut frame) },
            RStatus::Ok
        );
    }

    #[test]
    fn nested_root_frames_preserve_only_active_values() {
        let mut context = RimeraContext::default();
        let mut outer = operations::string(&mut context, "outer").unwrap();
        let mut inner = operations::string(&mut context, "inner").unwrap();
        let mut outer_frame = RRootFrame::new(&raw mut outer, 1);
        let mut inner_frame = RRootFrame::new(&raw mut inner, 1);
        assert_eq!(
            // SAFETY: both frames and their slots remain live for the test.
            unsafe { rimera_roots_push(&raw mut context, &raw mut outer_frame) },
            RStatus::Ok
        );
        assert_eq!(
            // SAFETY: the inner frame remains live above the outer frame.
            unsafe { rimera_roots_push(&raw mut context, &raw mut inner_frame) },
            RStatus::Ok
        );
        context.collect();
        assert_eq!(context.stats().live, 2);
        assert_eq!(
            // SAFETY: the inner frame is the active frame.
            unsafe { rimera_roots_pop(&raw mut context, &raw mut inner_frame) },
            RStatus::Ok
        );
        context.collect();
        assert_eq!(operations::display(&context, outer).unwrap(), "outer");
        assert!(operations::display(&context, inner).is_err());
        assert_eq!(
            // SAFETY: the outer frame is active after removing the inner one.
            unsafe { rimera_roots_pop(&raw mut context, &raw mut outer_frame) },
            RStatus::Ok
        );
    }

    #[test]
    fn unrooted_values_are_collected_and_generations_advance() {
        let mut context = RimeraContext::default();
        let stale = operations::string(&mut context, "gone").unwrap();
        context.collect();
        assert!(operations::display(&context, stale).is_err());
        assert_eq!(context.stats().live, 0);
    }

    #[test]
    fn allocation_threshold_triggers_collection() {
        let mut context = RimeraContext::default();
        let value = "x".repeat(4096);
        for _ in 0..17 {
            let _ = operations::string(&mut context, &value).unwrap();
        }
        assert_eq!(context.stats().collections, 1);
        assert!(context.stats().live <= 2);
        assert!(context.stats().next_collection_bytes >= 64 * 1024);
    }

    #[test]
    fn gate7_native_module_import_cache_namespace_and_failure_are_managed() {
        let mut context = RimeraContext::default();
        let inspect = context.import_name("inspect").unwrap();
        assert_eq!(inspect, context.import_name("inspect").unwrap());
        let weakref = context.import_name("weakref").unwrap();
        assert_ne!(inspect, weakref);

        let name = context.attribute_get(inspect, "__name__").unwrap();
        assert!(
            matches!(context.heap.get(name), Some(HeapObject::String(value)) if value == "inspect")
        );
        let dictionary = context.attribute_get(inspect, "__dict__").unwrap();
        assert!(matches!(
            context.heap.get(dictionary),
            Some(HeapObject::Dictionary(_))
        ));

        let marker = RValue::small_int(7);
        context.attribute_set(inspect, "marker", marker).unwrap();
        assert_eq!(context.attribute_get(inspect, "marker").unwrap(), marker);
        context.attribute_delete(inspect, "marker").unwrap();
        assert!(context.attribute_get(inspect, "marker").is_err());

        context.collect();
        assert_eq!(context.import_name("inspect").unwrap(), inspect);
        assert_eq!(
            context.import_name("math").unwrap_err(),
            "No module named 'math'"
        );
        assert!(context.consume_exception_type("ModuleNotFoundError"));
    }

    #[test]
    fn gate9_module_state_is_single_owner_validated_and_gc_managed() {
        let mut context = RimeraContext::default();
        let inspect = context.import_name("inspect").unwrap();
        assert!(matches!(
            context.heap.get(inspect),
            Some(HeapObject::Module(ModuleObject {
                state: ModuleState::Ready,
                ..
            }))
        ));
        assert_eq!(
            context
                .transition_module(inspect, ModuleState::Failed)
                .unwrap_err(),
            "illegal module state transition from Ready to Failed"
        );

        let namespace = context
            .allocate(HeapObject::Dictionary(crate::object::DictionaryObject {
                entries: Vec::new(),
            }))
            .unwrap();
        let failed = context
            .allocate(HeapObject::Module(ModuleObject {
                name: "failed".to_owned(),
                namespace,
                state: ModuleState::Created,
            }))
            .unwrap();
        context
            .transition_module(failed, ModuleState::Initializing)
            .unwrap();
        context
            .transition_module(failed, ModuleState::Failed)
            .unwrap();
        context.collect();
        assert!(context.heap.get(failed).is_none());
        assert!(matches!(
            context.heap.get(inspect),
            Some(HeapObject::Module(ModuleObject {
                state: ModuleState::Ready,
                ..
            }))
        ));
    }

    #[test]
    fn gate7_import_heap_failure_is_memory_error_and_publication_is_atomic() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let limit = context.stats().live_bytes;
        context.set_heap_limit(Some(limit)).unwrap();

        let mut output = RValue::NONE;
        let name = b"inspect";
        assert_eq!(
            unsafe {
                rimera_import_name(&raw mut context, name.as_ptr(), name.len(), &raw mut output)
            },
            RStatus::Exception
        );
        assert_eq!(output, RValue::NONE);
        assert!(context.consume_exception_type("MemoryError"));

        context.set_heap_limit(None).unwrap();
        assert_eq!(
            unsafe {
                rimera_import_name(&raw mut context, name.as_ptr(), name.len(), &raw mut output)
            },
            RStatus::Ok
        );
        let module_name = context.attribute_get(output, "__name__").unwrap();
        assert!(
            matches!(context.heap.get(module_name), Some(HeapObject::String(value)) if value == "inspect")
        );
        assert_eq!(context.import_name("inspect").unwrap(), output);
    }

    #[test]
    fn gate9_sys_modules_mutation_and_import_publication_are_low_heap_atomic() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let sys = context.import_name("sys").unwrap();
        let modules = context.attribute_get(sys, "modules").unwrap();
        context.collect();
        let long_name = "gate9_low_heap_cache_key_".repeat(512);
        let long_key = operations::string(&mut context, &long_name).unwrap();
        let limit = context.stats().live_bytes;
        context.set_heap_limit(Some(limit)).unwrap();

        assert_eq!(
            unsafe {
                rimera_item_set(
                    &raw mut context,
                    &raw const modules,
                    &raw const long_key,
                    &raw const sys,
                )
            },
            RStatus::Exception
        );
        assert!(context.consume_exception_type("MemoryError"));
        assert!(matches!(
            context.heap.get(modules),
            Some(HeapObject::Dictionary(dictionary)) if dictionary.get(&long_name).is_none()
        ));

        context.set_heap_limit(None).unwrap();
        let sys_key = operations::string(&mut context, "sys").unwrap();
        let replacement_limit = context.stats().live_bytes;
        context.set_heap_limit(Some(replacement_limit)).unwrap();

        assert_eq!(
            unsafe {
                rimera_item_set(
                    &raw mut context,
                    &raw const modules,
                    &raw const sys_key,
                    &raw const sys,
                )
            },
            RStatus::Ok
        );
        assert!(matches!(
            context.heap.get(modules),
            Some(HeapObject::Dictionary(dictionary)) if dictionary.get("sys") == Some(sys)
        ));
    }

    #[test]
    fn gate7_reflection_call_heap_failure_is_memory_error_and_source_namespace_survives() {
        let mut context = RimeraContext::default();
        let inspect = context.import_name("inspect").unwrap();
        let dir = context.ensure_builtin("dir").unwrap().unwrap();
        context.type_of(inspect).unwrap();
        let before_name = context.attribute_get(inspect, "__name__").unwrap();
        let limit = context.stats().live_bytes;
        context.set_heap_limit(Some(limit)).unwrap();

        let positional = [inspect];
        let arguments = RCallArguments {
            positional: positional.as_ptr(),
            positional_len: positional.len(),
            keywords: ptr::null(),
            keyword_len: 0,
        };
        let mut output = RValue::NONE;
        assert_eq!(
            unsafe {
                rimera_call(
                    &raw mut context,
                    &raw const dir,
                    &raw const arguments,
                    &raw mut output,
                )
            },
            RStatus::Exception
        );
        assert_eq!(output, RValue::NONE);
        assert!(context.consume_exception_type("MemoryError"));
        assert_eq!(
            context.attribute_get(inspect, "__name__").unwrap(),
            before_name
        );

        context.set_heap_limit(None).unwrap();
        assert_eq!(
            unsafe {
                rimera_call(
                    &raw mut context,
                    &raw const dir,
                    &raw const arguments,
                    &raw mut output,
                )
            },
            RStatus::Ok
        );
        assert!(matches!(
            context.heap.get(output),
            Some(HeapObject::List(_))
        ));
        assert_eq!(
            context.attribute_get(inspect, "__name__").unwrap(),
            before_name
        );
    }

    #[test]
    fn lazy_kernel_fits_the_public_low_heap_budget() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        assert!(context.builtin_type("BufferError").is_none());
        assert!(context.builtin_type("GeneratorExit").is_none());
        assert!(context.builtin_type("StopIteration").is_none());
        let stats = context.stats();
        assert!(
            stats.live_bytes <= 12 * 1024,
            "lazy kernel uses {} managed bytes",
            stats.live_bytes
        );
    }

    #[test]
    fn chr_surrogate_boundary_is_explicit_and_structured() {
        let mut context = RimeraContext::default();
        let chr = context.ensure_builtin("chr").unwrap().unwrap();
        let scalar = call::invoke(&mut context, chr, &[RValue::small_int(0xD7FF)], &[]).unwrap();
        assert!(
            matches!(context.heap.get(scalar), Some(HeapObject::String(text)) if text == "\u{D7FF}")
        );

        let error = call::invoke(&mut context, chr, &[RValue::small_int(0xD800)], &[]).unwrap_err();
        assert_eq!(
            error,
            "Rimera Gate 3 strings do not support lone surrogate code points"
        );
        assert!(context.consume_exception_type("ValueError"));
    }

    #[test]
    fn gate3_builtin_namespace_is_lazy_complete_and_first_class() {
        const REQUIRED: &[&str] = &[
            "abs",
            "all",
            "any",
            "ascii",
            "bin",
            "bool",
            "bytearray",
            "bytes",
            "callable",
            "chr",
            "classmethod",
            "complex",
            "delattr",
            "dict",
            "divmod",
            "enumerate",
            "filter",
            "float",
            "format",
            "frozenset",
            "getattr",
            "hasattr",
            "hash",
            "hex",
            "id",
            "int",
            "isinstance",
            "issubclass",
            "iter",
            "len",
            "list",
            "map",
            "max",
            "memoryview",
            "min",
            "next",
            "object",
            "oct",
            "ord",
            "pow",
            "print",
            "property",
            "range",
            "repr",
            "reversed",
            "round",
            "set",
            "setattr",
            "slice",
            "sorted",
            "staticmethod",
            "str",
            "sum",
            "super",
            "tuple",
            "type",
            "zip",
        ];

        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let builtins = context.builtins().unwrap();
        let Some(HeapObject::Dictionary(dictionary)) = context.heap.get(builtins) else {
            panic!("builtins dictionary is missing");
        };
        for lazy in ["abs", "int", "object", "type", "Ellipsis", "__debug__"] {
            assert!(dictionary.get(lazy).is_none(), "{lazy} should be lazy");
        }
        for internal in ["NoneType", "NotImplementedType", "dict_keys", "ellipsis"] {
            assert!(
                dictionary.get(internal).is_none(),
                "{internal} must not leak into builtins"
            );
        }
        assert!(context.builtin_type("ellipsis").is_none());

        for name in REQUIRED {
            assert!(
                context.ensure_builtin(name).unwrap().is_some(),
                "required Gate 3 builtin {name} is missing"
            );
        }
        for reflection in ["dir", "vars", "globals", "locals"] {
            assert!(
                context.ensure_builtin(reflection).unwrap().is_some(),
                "Gate 7 reflection builtin {reflection} is missing"
            );
        }
        assert!(
            context.ensure_builtin("__import__").unwrap().is_some(),
            "Gate 9 __import__ builtin is missing from the native namespace"
        );
        for excluded in ["compile", "eval", "exec"] {
            assert!(
                context.ensure_builtin(excluded).unwrap().is_none(),
                "unsupported builtin {excluded} leaked into the native namespace"
            );
        }

        let ellipsis = context.ensure_builtin("Ellipsis").unwrap().unwrap();
        assert_eq!(
            ellipsis,
            context.ensure_builtin("Ellipsis").unwrap().unwrap()
        );
        assert!(matches!(
            context.heap.get(ellipsis),
            Some(HeapObject::Ellipsis)
        ));
        let ellipsis_type = context.type_of(ellipsis).unwrap();
        assert_eq!(ellipsis_type, context.builtin_type("ellipsis").unwrap());
        assert_eq!(operations::display(&context, ellipsis).unwrap(), "Ellipsis");
        let Some(HeapObject::Dictionary(dictionary)) = context.heap.get(builtins) else {
            panic!("builtins dictionary disappeared");
        };
        assert!(dictionary.get("ellipsis").is_none());
        assert!(dictionary.get("NotImplementedType").is_none());

        let debug = context.ensure_builtin("__debug__").unwrap().unwrap();
        assert_eq!(debug, RValue::boolean(true));
        assert_eq!(context.ensure_builtin("__debug__").unwrap(), Some(debug));
    }

    #[test]
    fn gate3_float_and_complex_small_surfaces_use_managed_attributes() {
        let mut context = RimeraContext::default();
        let value = operations::float(&mut context, 3.5).unwrap();
        assert_eq!(context.attribute_get(value, "real").unwrap(), value);
        let imag = context.attribute_get(value, "imag").unwrap();
        assert!(
            matches!(context.heap.get(imag), Some(HeapObject::Float(number)) if *number == 0.0)
        );

        let conjugate = context.attribute_get(value, "conjugate").unwrap();
        assert_eq!(
            call::invoke(&mut context, conjugate, &[], &[]).unwrap(),
            value
        );
        let is_integer = context.attribute_get(value, "is_integer").unwrap();
        assert_eq!(
            call::invoke(&mut context, is_integer, &[], &[]).unwrap(),
            RValue::boolean(false)
        );
        let ratio = context.attribute_get(value, "as_integer_ratio").unwrap();
        let ratio = call::invoke(&mut context, ratio, &[], &[]).unwrap();
        assert_eq!(operations::display(&context, ratio).unwrap(), "(7, 2)");
        let hex = context.attribute_get(value, "hex").unwrap();
        let hex = call::invoke(&mut context, hex, &[], &[]).unwrap();
        assert_eq!(
            operations::display(&context, hex).unwrap(),
            "0x1.c000000000000p+1"
        );

        let number = operations::complex(&mut context, 3.0, -4.0).unwrap();
        let real = context.attribute_get(number, "real").unwrap();
        let imag = context.attribute_get(number, "imag").unwrap();
        assert!(matches!(context.heap.get(real), Some(HeapObject::Float(value)) if *value == 3.0));
        assert!(matches!(context.heap.get(imag), Some(HeapObject::Float(value)) if *value == -4.0));
        let conjugate = context.attribute_get(number, "conjugate").unwrap();
        let conjugate = call::invoke(&mut context, conjugate, &[], &[]).unwrap();
        assert!(matches!(
            context.heap.get(conjugate),
            Some(HeapObject::Complex { real, imag }) if *real == 3.0 && *imag == 4.0
        ));
    }

    #[test]
    fn ordered_hash_table_managed_size_keeps_retained_capacity_visible() {
        let mut table = crate::object::OrderedHashTable::<RValue>::default();
        for index in 0..256_usize {
            table.insert_new(index as i64, RValue::small_int(index as i64));
        }
        let populated = table.managed_size();
        for index in 0..256_usize {
            assert!(table.remove(index).is_some());
        }
        let retained = table.managed_size();
        assert!(populated > 256 * std::mem::size_of::<RValue>());
        assert!(retained > 256 * std::mem::size_of::<RValue>());
    }

    #[test]
    fn in_place_collection_growth_refreshes_managed_bytes_and_heap_limit() {
        let mut context = RimeraContext::default();
        let list = context.allocate(HeapObject::List(Vec::new())).unwrap();
        context.add_context_root(list);
        let before = context.stats().live_bytes;
        let limit = before + 4096;
        context.set_heap_limit(Some(limit)).unwrap();
        let status = protect(&raw mut context, |context| {
            let Some(HeapObject::List(values)) = context.heap.get_mut(list) else {
                panic!("rooted list disappeared");
            };
            for value in 0..1024_i64 {
                values.push(RValue::small_int(value));
            }
            Ok(())
        });
        assert_eq!(status, RStatus::Exception);
        assert!(context.stats().live_bytes > limit);
        assert!(context.stats().live_bytes >= 1024 * std::mem::size_of::<RValue>());
    }

    #[test]
    fn context_root_preserves_an_object_graph() {
        let mut context = RimeraContext::default();
        let child = operations::string(&mut context, "child").unwrap();
        let parent = context
            .allocate(HeapObject::ValueArray(vec![child].into_boxed_slice()))
            .unwrap();
        context.add_context_root(parent);
        context.collect();
        assert_eq!(context.stats().live, 2);
        assert_eq!(operations::display(&context, child).unwrap(), "child");
    }

    #[test]
    fn temporary_roots_survive_collection_and_are_removed_afterward() {
        let mut context = RimeraContext::default();
        let value = operations::string(&mut context, "temporary").unwrap();
        context.with_temporary_roots(&[value], |context| context.collect());
        assert_eq!(operations::display(&context, value).unwrap(), "temporary");
        assert_eq!(context.native_root_count(), 0);
        context.collect();
        assert!(operations::display(&context, value).is_err());
    }

    #[test]
    fn temporary_roots_are_removed_during_panic_unwinding() {
        let mut context = RimeraContext::default();
        let value = operations::string(&mut context, "temporary").unwrap();
        let panic = std::panic::catch_unwind(AssertUnwindSafe(|| {
            context.with_temporary_roots(&[value], |_| panic!("test panic"));
        }));
        assert!(panic.is_err());
        assert_eq!(context.native_root_count(), 0);
        context.collect();
        assert!(operations::display(&context, value).is_err());
    }

    #[test]
    fn heap_limit_collects_before_rejecting_allocation() {
        let mut context = RimeraContext::default();
        assert_eq!(
            // SAFETY: the stack context remains live for this call.
            unsafe { rimera_context_set_heap_limit(&raw mut context, 512) },
            RStatus::Ok
        );
        let mut rooted = operations::string(&mut context, &"x".repeat(128)).unwrap();
        let mut frame = RRootFrame::new(&raw mut rooted, 1);
        assert_eq!(
            // SAFETY: the context and frame remain live for the registration.
            unsafe { rimera_roots_push(&raw mut context, &raw mut frame) },
            RStatus::Ok
        );
        let result = operations::string(&mut context, &"y".repeat(128));
        assert_eq!(result.unwrap_err(), "managed heap limit exceeded");
        assert_eq!(
            // SAFETY: the frame is still the active frame.
            unsafe { rimera_roots_pop(&raw mut context, &raw mut frame) },
            RStatus::Ok
        );
    }

    #[test]
    fn floor_division_matches_python_for_negative_values() {
        let mut context = RimeraContext::default();
        let left = RValue::small_int(-7);
        let right = RValue::small_int(3);
        let quotient = operations::binary(&mut context, 3, left, right).unwrap();
        let remainder = operations::binary(&mut context, 4, left, right).unwrap();
        assert_eq!(operations::display(&context, quotient).unwrap(), "-3");
        assert_eq!(operations::display(&context, remainder).unwrap(), "2");
    }

    #[test]
    fn large_integers_use_managed_handles() {
        let mut context = RimeraContext::default();
        let value =
            operations::int_from_decimal(&mut context, "123456789012345678901234567890").unwrap();
        assert_eq!(value.tag, rimera_abi::RTag::Handle as u32);
        assert_eq!(
            operations::display(&context, value).unwrap(),
            "123456789012345678901234567890"
        );
    }

    #[test]
    fn kernel_objects_and_closure_cells_are_traced() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let value = operations::string(&mut context, "captured").unwrap();
        let mut cell = RValue::NONE;
        assert_eq!(
            unsafe { rimera_cell_new(&raw mut context, &raw const value, &raw mut cell) },
            RStatus::Ok
        );
        context.add_context_root(cell);
        context.collect();
        let mut read = RValue::NONE;
        assert_eq!(
            unsafe { rimera_cell_get(&raw mut context, &raw const cell, &raw mut read) },
            RStatus::Ok
        );
        assert_eq!(operations::display(&context, read).unwrap(), "captured");
    }

    #[test]
    fn generic_call_binds_full_parameter_kinds() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let names = [b"a".as_slice(), b"b", b"rest", b"flag", b"named"];
        let parameters = [
            RParameterSpec {
                name: names[0].as_ptr(),
                name_len: names[0].len(),
                kind: ParameterKind::PositionalOnly as u8,
                has_default: 0,
                reserved: [0; 6],
                default: RValue::NONE,
            },
            RParameterSpec {
                name: names[1].as_ptr(),
                name_len: names[1].len(),
                kind: ParameterKind::PositionalOrKeyword as u8,
                has_default: 1,
                reserved: [0; 6],
                default: RValue::small_int(7),
            },
            RParameterSpec {
                name: names[2].as_ptr(),
                name_len: names[2].len(),
                kind: ParameterKind::VarArgs as u8,
                has_default: 0,
                reserved: [0; 6],
                default: RValue::NONE,
            },
            RParameterSpec {
                name: names[3].as_ptr(),
                name_len: names[3].len(),
                kind: ParameterKind::KeywordOnly as u8,
                has_default: 1,
                reserved: [0; 6],
                default: RValue::boolean(false),
            },
            RParameterSpec {
                name: names[4].as_ptr(),
                name_len: names[4].len(),
                kind: ParameterKind::VarKeywords as u8,
                has_default: 0,
                reserved: [0; 6],
                default: RValue::NONE,
            },
        ];
        let mut function = RValue::NONE;
        assert_eq!(
            unsafe {
                rimera_function_new(
                    &raw mut context,
                    add_native as *const c_void,
                    b"add".as_ptr(),
                    3,
                    b"add".as_ptr(),
                    3,
                    parameters.as_ptr(),
                    parameters.len(),
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    &raw mut function,
                )
            },
            RStatus::Ok
        );
        let positional = [RValue::small_int(35)];
        let keyword = rimera_abi::RKeywordArgument {
            name: b"extra".as_ptr(),
            name_len: 5,
            value: RValue::small_int(9),
        };
        let arguments = RCallArguments {
            positional: positional.as_ptr(),
            positional_len: positional.len(),
            keywords: &raw const keyword,
            keyword_len: 1,
        };
        let mut output = RValue::NONE;
        assert_eq!(
            unsafe {
                rimera_call(
                    &raw mut context,
                    &raw const function,
                    &raw const arguments,
                    &raw mut output,
                )
            },
            RStatus::Ok
        );
        assert_eq!(operations::display(&context, output).unwrap(), "42");
    }

    #[test]
    fn gate7_slice4_code_metadata_allocation_is_low_heap_atomic() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let before = context.heap.live_bytes();
        context
            .set_heap_limit(Some(before.saturating_add(1)))
            .unwrap();

        let local = RNameSpec {
            name: b"value".as_ptr(),
            name_len: 5,
        };
        let metadata = RCodeMetadataSpec {
            filename: b"gate7_low_heap.py".as_ptr(),
            filename_len: 17,
            first_line: 7,
            reserved: 0,
            local_names: &raw const local,
            local_name_len: 1,
            cell_names: std::ptr::null(),
            cell_name_len: 0,
            free_names: std::ptr::null(),
            free_name_len: 0,
        };
        let mut output = RValue::small_int(2718);
        let status = unsafe {
            rimera_function_new(
                &raw mut context,
                add_native as *const c_void,
                b"low_heap".as_ptr(),
                8,
                b"low_heap".as_ptr(),
                8,
                std::ptr::null(),
                0,
                std::ptr::null(),
                0,
                &raw const metadata,
                &raw mut output,
            )
        };
        assert_eq!(status, RStatus::Exception);
        assert_eq!(output, RValue::small_int(2718));
        let raised = context
            .raised
            .expect("code metadata allocation should raise MemoryError");
        assert_eq!(
            context.type_of(raised).unwrap(),
            context.builtin_type("MemoryError").unwrap()
        );
        context.collect();
        assert!(context.heap.live_bytes() <= before);
    }

    #[test]
    fn gate7_slice5_traceback_frame_allocation_is_low_heap_atomic() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let mut original = context.raise_builtin("ValueError", "boom").unwrap();
        let mut frame = RRootFrame::new(&raw mut original, 1);
        assert_eq!(
            unsafe { rimera_roots_push(&raw mut context, &raw mut frame) },
            RStatus::Ok
        );
        let before = context.heap.live_bytes();
        context
            .set_heap_limit(Some(before.saturating_add(1)))
            .unwrap();

        let status = unsafe {
            rimera_traceback_append(
                &raw mut context,
                b"gate7_low_heap.py".as_ptr(),
                17,
                b"<module>".as_ptr(),
                8,
                1,
                0,
            )
        };
        assert_eq!(status, RStatus::Exception);
        let raised = context
            .raised
            .expect("traceback metadata allocation should raise MemoryError");
        assert_eq!(
            context.type_of(raised).unwrap(),
            context.builtin_type("MemoryError").unwrap()
        );
        assert_eq!(
            match context.heap.get(original) {
                Some(HeapObject::Exception(exception)) => exception.traceback,
                _ => Some(RValue::NONE),
            },
            None
        );
        context.collect();
        assert!(context.heap.get(original).is_some());
        assert_eq!(
            unsafe { rimera_roots_pop(&raw mut context, &raw mut frame) },
            RStatus::Ok
        );
    }

    #[test]
    fn gate7_slice6_generator_frame_publication_is_low_heap_atomic() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let mut function = test_function(
            &mut context,
            two_stage_generator_resume as usize,
            FunctionKind::Generator {
                persistent_slot_count: 1,
            },
            "low_heap_generator",
            "low_heap_generator",
            Vec::new(),
            &[],
        );
        let mut root = RRootFrame::new(&raw mut function, 1);
        assert_eq!(
            unsafe { rimera_roots_push(&raw mut context, &raw mut root) },
            RStatus::Ok
        );
        let before = context.stats();
        context.set_heap_limit(Some(before.live_bytes)).unwrap();
        let arguments = RCallArguments {
            positional: std::ptr::null(),
            positional_len: 0,
            keywords: std::ptr::null(),
            keyword_len: 0,
        };
        let mut output = RValue::small_int(31415);
        assert_eq!(
            unsafe {
                rimera_call(
                    &raw mut context,
                    &raw const function,
                    &raw const arguments,
                    &raw mut output,
                )
            },
            RStatus::Exception
        );
        assert_eq!(output, RValue::small_int(31415));
        let raised = context
            .raised
            .expect("generator metadata allocation should raise MemoryError");
        assert_eq!(context.exception_type_name(raised), Some("MemoryError"));
        assert_eq!(context.stats().live, before.live);
        context.collect();
        assert!(context.heap.get(function).is_some());
        assert_eq!(
            unsafe { rimera_roots_pop(&raw mut context, &raw mut root) },
            RStatus::Ok
        );
        context.set_heap_limit(None).unwrap();
    }

    #[test]
    fn exception_state_tracks_handler_context_and_traceback() {
        let mut context = RimeraContext::default();
        let first = context.raise_builtin("ValueError", "inner").unwrap();
        assert_eq!(context.handler_enter().unwrap(), first);
        let second = context.raise_builtin("RuntimeError", "outer").unwrap();
        assert_eq!(
            match context.heap.get(second) {
                Some(HeapObject::Exception(exception)) => exception.context,
                _ => None,
            },
            Some(first)
        );
        context
            .attach_traceback("program.py", "worker", 12, 4)
            .unwrap();
        let rendered = context.render_active_exception();
        assert!(rendered.contains("File \"program.py\", line 12, in worker"));
        assert!(rendered.ends_with("RuntimeError: outer"));
    }

    #[test]
    fn gate5_exception_graphs_survive_low_heap_collection_and_failed_metadata_is_atomic() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let exception_type = context
            .ensure_builtin_type("RuntimeError")
            .unwrap()
            .unwrap();
        let mut exception = context.new_exception(exception_type, &[]).unwrap();
        let mut frame = RRootFrame::new(&raw mut exception, 1);
        assert_eq!(
            unsafe { rimera_roots_push(&raw mut context, &raw mut frame) },
            RStatus::Ok
        );
        context.attribute_set(exception, "peer", exception).unwrap();
        let Some(HeapObject::Exception(object)) = context.heap.get_mut(exception) else {
            panic!("expected managed exception");
        };
        object.cause = Some(exception);
        object.context = Some(exception);
        let live = context.stats().live_bytes;
        context
            .set_heap_limit(Some(live.saturating_add(4_096)))
            .unwrap();
        let collections_before = context.stats().collections;
        for _ in 0..64 {
            let _ = context.new_exception(exception_type, &[]).unwrap();
        }
        assert!(context.heap.get(exception).is_some());
        assert!(context.stats().collections > collections_before);
        context.set_heap_limit(None).unwrap();
        assert_eq!(
            unsafe { rimera_roots_pop(&raw mut context, &raw mut frame) },
            RStatus::Ok
        );
        context.collect();
        assert!(context.heap.get(exception).is_none());

        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let exception_type = context
            .ensure_builtin_type("RuntimeError")
            .unwrap()
            .unwrap();
        let mut exception = context.new_exception(exception_type, &[]).unwrap();
        let mut frame = RRootFrame::new(&raw mut exception, 1);
        assert_eq!(
            unsafe { rimera_roots_push(&raw mut context, &raw mut frame) },
            RStatus::Ok
        );
        let live = context.stats().live_bytes;
        context.set_heap_limit(Some(live)).unwrap();
        assert_eq!(
            context.attribute_set(exception, "new_field", RValue::small_int(1)),
            Err("managed heap limit exceeded".to_owned())
        );
        let Some(HeapObject::Exception(object)) = context.heap.get(exception) else {
            panic!("exception must remain valid after failed metadata allocation");
        };
        assert!(object.dictionary.is_none());
        assert_eq!(
            unsafe { rimera_roots_pop(&raw mut context, &raw mut frame) },
            RStatus::Ok
        );
    }

    #[test]
    fn terminal_tracebacks_emphasize_headers_and_exception_summaries() {
        let rendered = "Traceback (most recent call last):\n  File \"program.py\", line 1, in <module>\nZeroDivisionError: integer division or modulo by zero";
        assert_eq!(
            format_runtime_error(rendered, true),
            "\x1b[1;31mTraceback (most recent call last):\x1b[0m\n  File \"program.py\", line 1, in <module>\n\x1b[1;31mZeroDivisionError: integer division or modulo by zero\x1b[0m"
        );
        assert_eq!(format_runtime_error(rendered, false), rendered);
    }

    #[test]
    fn type_kernel_is_lazy_exact_and_gc_rooted() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        for name in [
            "NoneType", "int", "bool", "str", "list", "tuple", "dict", "set",
        ] {
            assert!(
                context.builtin_type(name).is_none(),
                "{name} should be lazy"
            );
        }
        assert!(context.builtin_type("builtin_function_or_method").is_none());
        let object = context.builtin_type("object").unwrap();
        assert_eq!(
            context.type_of(RValue::NONE).unwrap(),
            context.builtin_type("NoneType").unwrap()
        );
        assert_eq!(
            context.type_of(RValue::small_int(1)).unwrap(),
            context.builtin_type("int").unwrap()
        );
        assert_eq!(
            context.type_of(RValue::boolean(true)).unwrap(),
            context.builtin_type("bool").unwrap()
        );
        let big =
            operations::int_from_decimal(&mut context, "123456789012345678901234567890").unwrap();
        assert_eq!(
            context.type_of(big).unwrap(),
            context.builtin_type("int").unwrap()
        );
        let text = operations::string(&mut context, "rimera").unwrap();
        assert_eq!(
            context.type_of(text).unwrap(),
            context.builtin_type("str").unwrap()
        );
        let list = operations::list(&mut context, &[RValue::small_int(1)]).unwrap();
        assert_eq!(
            context.type_of(list).unwrap(),
            context.builtin_type("list").unwrap()
        );
        let tuple = context
            .allocate(HeapObject::Tuple(
                vec![RValue::small_int(1)].into_boxed_slice(),
            ))
            .unwrap();
        assert_eq!(
            context.type_of(tuple).unwrap(),
            context.builtin_type("tuple").unwrap()
        );
        let dictionary =
            operations::dictionary(&mut context, &[text], &[RValue::small_int(1)]).unwrap();
        assert_eq!(
            context.type_of(dictionary).unwrap(),
            context.builtin_type("dict").unwrap()
        );
        let internal_dictionary = context
            .allocate(HeapObject::Dictionary(crate::object::DictionaryObject {
                entries: Vec::new(),
            }))
            .unwrap();
        assert_eq!(
            context.type_of(internal_dictionary).unwrap(),
            context.builtin_type("dict").unwrap()
        );
        let set = operations::set(&mut context, &[RValue::small_int(1)]).unwrap();
        assert_eq!(
            context.type_of(set).unwrap(),
            context.builtin_type("set").unwrap()
        );
        let range = operations::range(
            &mut context,
            RValue::small_int(0),
            RValue::small_int(1),
            RValue::small_int(1),
        )
        .unwrap();
        assert_eq!(
            context.type_of(range).unwrap(),
            context.builtin_type("range").unwrap()
        );
        let iterator = operations::iterator_new(&mut context, range).unwrap();
        assert_eq!(
            context.type_of(iterator).unwrap(),
            context.builtin_type("iterator").unwrap()
        );
        let cell = context
            .allocate(HeapObject::Cell(crate::object::CellObject { value: None }))
            .unwrap();
        assert_eq!(
            context.type_of(cell).unwrap(),
            context.builtin_type("cell").unwrap()
        );
        let traceback = context
            .allocate(HeapObject::Traceback(TracebackObject {
                filename: "type_kernel.py".to_owned(),
                function: "<module>".to_owned(),
                line: 1,
                column: 0,
                frame: None,
                next: None,
            }))
            .unwrap();
        assert_eq!(
            context.type_of(traceback).unwrap(),
            context.builtin_type("traceback").unwrap()
        );
        let function = test_function(
            &mut context,
            add_native as usize,
            FunctionKind::Normal,
            "native",
            "native",
            Vec::new(),
            &[],
        );
        assert_eq!(
            context.type_of(function).unwrap(),
            context.builtin_type("function").unwrap()
        );
        let builtin = context.ensure_builtin("isinstance").unwrap().unwrap();
        assert_eq!(
            context.type_of(builtin).unwrap(),
            context.builtin_type("builtin_function_or_method").unwrap()
        );
        let internal_array =
            operations::value_array(&mut context, &[RValue::small_int(1)]).unwrap();
        assert_eq!(context.type_of(internal_array).unwrap(), object);
        let exception = context.raise_builtin("ValueError", "bad").unwrap();
        assert_eq!(
            context.type_of(exception).unwrap(),
            context.builtin_type("ValueError").unwrap()
        );
        assert_eq!(
            context.type_of(object).unwrap(),
            context.builtin_type("type").unwrap()
        );
        let int = context.builtin_type("int").unwrap();
        let bool_type = context.builtin_type("bool").unwrap();
        assert!(context.is_subclass(bool_type, int).unwrap());
        assert_eq!(context.type_of(RValue::small_int(2)).unwrap(), int);
        context.collect();
        assert_eq!(context.type_of(RValue::small_int(3)).unwrap(), int);
        assert!(matches!(
            context.heap.get(builtin),
            Some(HeapObject::BuiltinFunction(_))
        ));
        assert!(context.builtin_type("object").is_some());
        assert_ne!(object, int);
    }

    #[test]
    fn builtin_type_calls_follow_generic_call_and_class_info_rules() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let type_callable = context.ensure_builtin("type").unwrap().unwrap();
        let int = context.ensure_builtin("int").unwrap().unwrap();
        let bool_type = context.ensure_builtin("bool").unwrap().unwrap();
        let nested_tuple = context
            .allocate(HeapObject::Tuple(vec![bool_type].into_boxed_slice()))
            .unwrap();
        let tuple = context
            .allocate(HeapObject::Tuple(
                vec![int, nested_tuple].into_boxed_slice(),
            ))
            .unwrap();
        let short_circuit = context
            .allocate(HeapObject::Tuple(
                vec![int, RValue::small_int(42)].into_boxed_slice(),
            ))
            .unwrap();
        let isinstance = context.ensure_builtin("isinstance").unwrap().unwrap();
        let issubclass = context.ensure_builtin("issubclass").unwrap().unwrap();
        assert_eq!(
            call::invoke(&mut context, type_callable, &[RValue::boolean(true)], &[]).unwrap(),
            bool_type
        );
        assert_eq!(
            call::invoke(
                &mut context,
                isinstance,
                &[RValue::boolean(true), tuple],
                &[],
            )
            .unwrap(),
            RValue::boolean(true)
        );
        assert_eq!(
            call::invoke(
                &mut context,
                isinstance,
                &[RValue::boolean(true), short_circuit],
                &[],
            )
            .unwrap(),
            RValue::boolean(true)
        );
        assert_eq!(
            call::invoke(&mut context, issubclass, &[bool_type, int], &[],).unwrap(),
            RValue::boolean(true)
        );
        assert_eq!(
            call::invoke(&mut context, type_callable, &[], &[]).unwrap_err(),
            "type() takes 1 or 3 arguments"
        );
        assert_eq!(
            call::invoke(
                &mut context,
                type_callable,
                &[RValue::small_int(1)],
                &[("value".to_owned(), RValue::small_int(2))],
            )
            .unwrap_err(),
            "type() takes 1 or 3 arguments"
        );
        assert_eq!(
            call::invoke(
                &mut context,
                isinstance,
                &[RValue::small_int(1), RValue::small_int(42)],
                &[],
            )
            .unwrap_err(),
            "isinstance() arg 2 must be a type, a tuple of types, or a union"
        );
        assert_eq!(
            call::invoke(
                &mut context,
                isinstance,
                &[RValue::small_int(1), int],
                &[("class_info".to_owned(), int)],
            )
            .unwrap_err(),
            "isinstance() takes no keyword arguments"
        );
        assert_eq!(
            call::invoke(&mut context, issubclass, &[RValue::small_int(1), int], &[],).unwrap_err(),
            "issubclass() arg 1 must be a class"
        );
        assert_eq!(
            call::invoke(
                &mut context,
                issubclass,
                &[bool_type, RValue::small_int(42)],
                &[]
            )
            .unwrap_err(),
            "issubclass() arg 2 must be a class, a tuple of classes, or a union"
        );
    }

    #[test]
    fn user_classes_have_stable_identity_instances_and_mro() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let namespace = context
            .allocate(HeapObject::ValueDictionary(ValueDictionaryObject::default()))
            .unwrap();
        let class = context.new_class("Empty", &[], namespace).unwrap();
        let first = context.new_instance(class).unwrap();
        let second = context.new_instance(class).unwrap();
        let object = context.builtin_type("object").unwrap();
        let type_type = context.builtin_type("type").unwrap();

        let HeapObject::Type(class_object) = context.heap.get(class).unwrap() else {
            panic!("class constructor did not produce a type");
        };
        assert_eq!(class_object.name, "Empty");
        assert_eq!(class_object.qualified_name, "Empty");
        assert_eq!(
            context
                .namespace_value(namespace, "__module__")
                .and_then(|value| match context.heap.get(value) {
                    Some(HeapObject::String(module)) => Some(module.as_str()),
                    _ => None,
                }),
            Some("__main__")
        );
        assert_eq!(
            operations::display(&context, class).unwrap(),
            "<class '__main__.Empty'>"
        );
        assert_eq!(class_object.bases.as_ref(), &[object]);
        assert_eq!(class_object.mro.as_ref(), &[class, object]);
        assert_eq!(context.type_of(class).unwrap(), type_type);
        assert_eq!(context.type_of(first).unwrap(), class);
        assert_eq!(context.type_of(second).unwrap(), class);
        assert!(context.is_instance(first, class).unwrap());
        assert!(context.is_instance(first, object).unwrap());
        assert!(context.is_subclass(class, object).unwrap());

        let first_dictionary = match context.heap.get(first) {
            Some(HeapObject::Instance(instance)) => instance.dictionary,
            _ => panic!("class call did not produce an instance"),
        };
        let second_dictionary = match context.heap.get(second) {
            Some(HeapObject::Instance(instance)) => instance.dictionary,
            _ => panic!("class call did not produce an instance"),
        };
        assert_ne!(first_dictionary, second_dictionary);

        context.with_temporary_roots(&[class, first, second], |context| context.collect());
        assert!(matches!(
            context.heap.get(first),
            Some(HeapObject::Instance(_))
        ));
        assert!(matches!(
            context.heap.get(second),
            Some(HeapObject::Instance(_))
        ));
    }

    #[test]
    fn user_class_instance_cycles_are_collectible() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let namespace = context
            .allocate(HeapObject::ValueDictionary(ValueDictionaryObject::default()))
            .unwrap();
        let class = context.new_class("Cycle", &[], namespace).unwrap();
        let instance = context.new_instance(class).unwrap();
        let key = operations::string(&mut context, "instance").unwrap();
        let hash = operations::hash_i64(&mut context, key).unwrap();
        let Some(HeapObject::ValueDictionary(dictionary)) = context.heap.get_mut(namespace) else {
            panic!("class namespace is not a dictionary");
        };
        dictionary.table.insert_new(hash, (key, instance));

        context.collect();
        assert!(context.heap.get(class).is_none());
        assert!(context.heap.get(instance).is_none());
        assert!(context.heap.get(namespace).is_none());
    }

    #[test]
    fn slots_install_traced_member_descriptors_and_inherit_storage() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let parent_namespace = context.namespace_new().unwrap();
        let left = operations::string(&mut context, "left").unwrap();
        let slots = context
            .allocate(HeapObject::Tuple(vec![left].into_boxed_slice()))
            .unwrap();
        context
            .namespace_set(parent_namespace, "__slots__", slots)
            .unwrap();
        let parent = context.new_class("Parent", &[], parent_namespace).unwrap();

        let child_namespace = context.namespace_new().unwrap();
        let right = operations::string(&mut context, "right").unwrap();
        context
            .namespace_set(child_namespace, "__slots__", right)
            .unwrap();
        let child = context
            .new_class("Child", &[parent], child_namespace)
            .unwrap();
        let instance = context.new_instance(child).unwrap();
        let value = operations::string(&mut context, "live through collection").unwrap();
        context.attribute_set(instance, "left", value).unwrap();
        context
            .attribute_set(instance, "right", RValue::small_int(42))
            .unwrap();
        assert_eq!(context.attribute_get(instance, "left").unwrap(), value);
        assert_eq!(
            context.attribute_get(instance, "right").unwrap(),
            RValue::small_int(42)
        );
        assert_eq!(
            context
                .attribute_set(instance, "missing", RValue::NONE)
                .unwrap_err(),
            "'Child' object has no attribute 'missing'"
        );
        context.with_temporary_roots(&[parent, child, instance], |context| context.collect());
        assert_eq!(context.attribute_get(instance, "left").unwrap(), value);
        context.attribute_delete(instance, "left").unwrap();
        assert_eq!(
            context.attribute_get(instance, "left").unwrap_err(),
            "'Child' object has no attribute 'left'"
        );
    }

    #[test]
    fn class_constructor_and_calls_share_the_generic_call_path() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let type_callable = context.ensure_builtin("type").unwrap().unwrap();
        let name = operations::string(&mut context, "Dynamic").unwrap();
        let bases = context.allocate(HeapObject::Tuple(Box::new([]))).unwrap();
        let namespace = context
            .allocate(HeapObject::ValueDictionary(ValueDictionaryObject::default()))
            .unwrap();
        let class =
            call::invoke(&mut context, type_callable, &[name, bases, namespace], &[]).unwrap();
        let instance = call::invoke(&mut context, class, &[], &[]).unwrap();
        assert_eq!(context.type_of(instance).unwrap(), class);
        assert_eq!(
            call::invoke(&mut context, class, &[RValue::small_int(1)], &[]).unwrap_err(),
            "Dynamic() takes no arguments"
        );
        assert_eq!(
            call::invoke(
                &mut context,
                type_callable,
                &[RValue::small_int(1), bases, namespace],
                &[],
            )
            .unwrap_err(),
            "type.__new__() argument 1 must be str, not int"
        );

        let object = context.builtin_type("object").unwrap();
        let object_bases = context
            .allocate(HeapObject::Tuple(vec![object].into_boxed_slice()))
            .unwrap();
        let explicit_object = call::invoke(
            &mut context,
            type_callable,
            &[name, object_bases, namespace],
            &[],
        )
        .unwrap();
        assert!(context.is_subclass(explicit_object, object).unwrap());
    }

    #[test]
    fn class_constructor_abi_creates_a_rooted_native_type() {
        let mut context = RimeraContext::default();
        let namespace = context
            .allocate(HeapObject::ValueDictionary(ValueDictionaryObject::default()))
            .unwrap();
        let name = b"AbiClass";
        let mut class = RValue::NONE;
        assert_eq!(
            // SAFETY: all buffers and the context remain live throughout the call.
            unsafe {
                rimera_class_new(
                    &raw mut context,
                    name.as_ptr(),
                    name.len(),
                    std::ptr::null(),
                    0,
                    &raw const namespace,
                    &raw mut class,
                )
            },
            RStatus::Ok
        );
        context.with_temporary_roots(&[class], |context| context.collect());
        assert!(matches!(context.heap.get(class), Some(HeapObject::Type(_))));
    }

    #[test]
    fn attributes_and_bound_methods_are_traced_and_collectible() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let namespace = context.namespace_new().unwrap();
        let function = test_function(
            &mut context,
            add_native as usize,
            FunctionKind::Normal,
            "method",
            "Holder.method",
            Vec::new(),
            &[],
        );
        context
            .namespace_set(namespace, "method", function)
            .unwrap();
        let class = context.new_class("Holder", &[], namespace).unwrap();
        let instance = context.new_instance(class).unwrap();
        let before_version = match context.heap.get(class) {
            Some(HeapObject::Type(class)) => class.version_tag,
            _ => panic!("class constructor did not produce a type"),
        };
        context
            .attribute_set(class, "category", RValue::small_int(1))
            .unwrap();
        let after_version = match context.heap.get(class) {
            Some(HeapObject::Type(class)) => class.version_tag,
            _ => panic!("class constructor did not produce a type"),
        };
        assert_eq!(after_version, before_version + 1);
        let bound = context.attribute_get(instance, "method").unwrap();
        assert!(matches!(
            context.heap.get(bound),
            Some(HeapObject::BoundMethod(_))
        ));
        context.attribute_set(instance, "bound", bound).unwrap();
        context.with_temporary_roots(&[bound], |context| context.collect());
        assert!(matches!(
            context.heap.get(instance),
            Some(HeapObject::Instance(_))
        ));
        assert!(matches!(
            context.heap.get(function),
            Some(HeapObject::Function(_))
        ));

        context.collect();
        assert!(context.heap.get(bound).is_none());
        assert!(context.heap.get(instance).is_none());
        assert!(context.heap.get(class).is_none());
    }

    #[test]
    fn gate8_special_method_lookup_uses_type_mro_descriptor_binding_and_abi_errors() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let namespace = context.namespace_new().unwrap();
        let function = test_function(
            &mut context,
            add_native as usize,
            FunctionKind::Normal,
            "__enter__",
            "Manager.__enter__",
            Vec::new(),
            &[],
        );
        context
            .namespace_set(namespace, "__enter__", function)
            .unwrap();
        let class = context.new_class("Manager", &[], namespace).unwrap();
        let instance = context.new_instance(class).unwrap();
        let name = b"__enter__";
        let mut method = RValue::NONE;
        assert_eq!(
            // SAFETY: the context, input value, name bytes, and output remain live.
            unsafe {
                rimera_special_method_get(
                    &raw mut context,
                    &raw const instance,
                    name.as_ptr(),
                    name.len(),
                    &raw mut method,
                )
            },
            RStatus::Ok
        );
        assert!(matches!(
            context.heap.get(method),
            Some(HeapObject::BoundMethod(bound))
                if bound.receiver == instance && bound.function == function
        ));
        context.with_temporary_roots(&[method], |context| context.collect());
        assert!(context.heap.get(instance).is_some());
        assert!(context.heap.get(function).is_some());

        let missing = b"__exit__";
        assert_eq!(
            // SAFETY: the same live ABI storage is reused for the missing lookup.
            unsafe {
                rimera_special_method_get(
                    &raw mut context,
                    &raw const instance,
                    missing.as_ptr(),
                    missing.len(),
                    &raw mut method,
                )
            },
            RStatus::Exception
        );
        let raised = context
            .raised
            .expect("missing special method raises TypeError");
        assert_eq!(context.exception_type_name(raised), Some("TypeError"));
        assert_eq!(
            context.exception_message(raised).as_deref(),
            Some(
                "'Manager' object does not support the context manager protocol (missed __exit__ method)"
            )
        );
    }

    #[test]
    fn gate7_reflective_type_mutation_invalidates_descendants_and_is_low_heap_atomic() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();

        let base_namespace = context.namespace_new().unwrap();
        let base = context.new_class("Base", &[], base_namespace).unwrap();
        let child_namespace = context.namespace_new().unwrap();
        let child = context
            .new_class("Child", &[base], child_namespace)
            .unwrap();
        let grand_namespace = context.namespace_new().unwrap();
        let grand = context
            .new_class("Grand", &[child], grand_namespace)
            .unwrap();

        let versions = |context: &RimeraContext| {
            [base, child, grand].map(|value| match context.heap.get(value) {
                Some(HeapObject::Type(class)) => class.version_tag,
                _ => panic!("expected a type object"),
            })
        };
        let before = versions(&context);
        context
            .attribute_set(base, "marker", RValue::small_int(7))
            .unwrap();
        let after = versions(&context);
        assert_eq!(after, before.map(|version| version + 1));
        let instance = context.new_instance(grand).unwrap();
        assert_eq!(
            context.attribute_get(instance, "marker").unwrap(),
            RValue::small_int(7)
        );

        let base_namespace = match context.heap.get(base) {
            Some(HeapObject::Type(class)) => class.namespace,
            _ => panic!("expected a type object"),
        };
        let version_before_failure = versions(&context);
        let limit = context.stats().live_bytes;
        context.set_heap_limit(Some(limit)).unwrap();
        let long_name = "reflective_low_heap_".repeat(512);
        let error = context
            .with_temporary_roots(&[base, child, grand, instance], |context| {
                context.attribute_set(base, &long_name, RValue::small_int(9))
            })
            .unwrap_err();
        assert!(error.contains("managed heap limit exceeded"));
        assert!(
            context
                .namespace_value(base_namespace, &long_name)
                .is_none()
        );
        assert_eq!(versions(&context), version_before_failure);
        context.set_heap_limit(None).unwrap();
    }

    #[test]
    fn gate7_reflective_string_metadata_growth_is_low_heap_atomic() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();

        let class_namespace = context.namespace_new().unwrap();
        let class = context.new_class("Compact", &[], class_namespace).unwrap();
        let function = test_function(
            &mut context,
            add_native as usize,
            FunctionKind::Normal,
            "compact",
            "compact",
            Vec::new(),
            &[],
        );
        let generator_function = test_function(
            &mut context,
            two_stage_generator_resume as usize,
            FunctionKind::Generator {
                persistent_slot_count: 0,
            },
            "compact_gen",
            "compact_gen",
            Vec::new(),
            &[],
        );
        let generator = call::invoke(&mut context, generator_function, &[], &[]).unwrap();
        let long_text = operations::string(&mut context, &"metadata-growth-".repeat(512)).unwrap();
        let roots = [class, function, generator_function, generator, long_text];

        context.with_temporary_roots(&roots, |context| {
            let limit = context.stats().live_bytes;
            context.set_heap_limit(Some(limit)).unwrap();

            let class_version = match context.heap.get(class) {
                Some(HeapObject::Type(class)) => class.version_tag,
                _ => panic!("expected class"),
            };
            let error = context
                .attribute_set(class, "__name__", long_text)
                .unwrap_err();
            assert!(error.contains("managed heap limit exceeded"));
            match context.heap.get(class) {
                Some(HeapObject::Type(class)) => {
                    assert_eq!(class.name, "Compact");
                    assert_eq!(class.version_tag, class_version);
                }
                _ => panic!("expected class"),
            }

            let error = context
                .attribute_set(function, "__qualname__", long_text)
                .unwrap_err();
            assert!(error.contains("managed heap limit exceeded"));
            match context.heap.get(function) {
                Some(HeapObject::Function(function)) => {
                    assert_eq!(function.qualified_name, "compact")
                }
                _ => panic!("expected function"),
            }

            let error = context
                .attribute_set(generator, "__name__", long_text)
                .unwrap_err();
            assert!(error.contains("managed heap limit exceeded"));
            match context.heap.get(generator) {
                Some(HeapObject::Generator(generator)) => {
                    assert!(generator.name_override.is_none());
                    match context.heap.get(generator.function) {
                        Some(HeapObject::Function(function)) => {
                            assert_eq!(function.name, "compact_gen")
                        }
                        _ => panic!("expected generator function"),
                    }
                }
                _ => panic!("expected generator"),
            }

            context.set_heap_limit(None).unwrap();
        });
    }

    #[test]
    fn user_inheritance_uses_c3_and_super_values_trace_the_hierarchy() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();

        let root_namespace = context.namespace_new().unwrap();
        context
            .namespace_set(root_namespace, "choice", RValue::small_int(1))
            .unwrap();
        let root = context.new_class("Root", &[], root_namespace).unwrap();

        let left_namespace = context.namespace_new().unwrap();
        context
            .namespace_set(left_namespace, "choice", RValue::small_int(2))
            .unwrap();
        let left = context.new_class("Left", &[root], left_namespace).unwrap();

        let right_namespace = context.namespace_new().unwrap();
        context
            .namespace_set(right_namespace, "choice", RValue::small_int(3))
            .unwrap();
        let right = context
            .new_class("Right", &[root], right_namespace)
            .unwrap();

        let diamond_namespace = context.namespace_new().unwrap();
        let diamond = context
            .new_class("Diamond", &[left, right], diamond_namespace)
            .unwrap();
        let object = context.builtin_type("object").unwrap();
        let Some(HeapObject::Type(diamond_type)) = context.heap.get(diamond) else {
            panic!("diamond class was not allocated as a type")
        };
        assert_eq!(
            diamond_type.mro.as_ref(),
            &[diamond, left, right, root, object]
        );
        assert!(context.is_subclass(diamond, root).unwrap());

        let instance = context.new_instance(diamond).unwrap();
        assert!(context.is_instance(instance, left).unwrap());
        assert_eq!(
            context.attribute_get(instance, "choice").unwrap(),
            RValue::small_int(2)
        );
        let after_left = context.new_super(left, instance).unwrap();
        assert_eq!(
            context.attribute_get(after_left, "choice").unwrap(),
            RValue::small_int(3)
        );

        let mut abi_super = RValue::NONE;
        assert_eq!(
            // SAFETY: the context and all three ABI values remain live for the call.
            unsafe {
                rimera_super_new(
                    &raw mut context,
                    &raw const left,
                    &raw const instance,
                    &raw mut abi_super,
                )
            },
            RStatus::Ok
        );
        context.with_temporary_roots(&[abi_super], |context| context.collect());
        assert!(context.heap.get(diamond).is_some());
        assert!(context.heap.get(root).is_some());

        context.collect();
        assert!(context.heap.get(abi_super).is_none());
        assert!(context.heap.get(diamond).is_none());
        assert!(context.heap.get(root).is_none());
    }

    #[test]
    fn c3_rejects_duplicate_and_inconsistent_user_bases() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let x_namespace = context.namespace_new().unwrap();
        let x = context.new_class("X", &[], x_namespace).unwrap();
        let y_namespace = context.namespace_new().unwrap();
        let y = context.new_class("Y", &[], y_namespace).unwrap();

        let duplicate_namespace = context.namespace_new().unwrap();
        assert_eq!(
            context
                .new_class("Duplicate", &[x, x], duplicate_namespace)
                .unwrap_err(),
            "duplicate base class X"
        );

        let a_namespace = context.namespace_new().unwrap();
        let a = context.new_class("A", &[x, y], a_namespace).unwrap();
        let b_namespace = context.namespace_new().unwrap();
        let b = context.new_class("B", &[y, x], b_namespace).unwrap();
        let conflict_namespace = context.namespace_new().unwrap();
        assert_eq!(
            context
                .new_class("Conflict", &[a, b], conflict_namespace)
                .unwrap_err(),
            "Cannot create a consistent method resolution\norder (MRO) for bases X, Y"
        );
    }

    #[test]
    fn exception_group_split_preserves_nested_subgroup_shape() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let value_error = context.raise_builtin("ValueError", "value").unwrap();
        context.raised = None;
        let type_error = context.raise_builtin("TypeError", "type").unwrap();
        context.raised = None;
        let message = context
            .allocate(HeapObject::String("group".to_owned()))
            .unwrap();
        let group_type = context.builtin_type("ExceptionGroup").unwrap();
        let group = context
            .new_exception_group(group_type, message, &[value_error, type_error])
            .unwrap();
        let expected = context.builtin_type("ValueError").unwrap();
        let (matched, rest) = context.split_exception(group, expected).unwrap();
        let matched = matched.expect("matching subgroup");
        let rest = rest.expect("unmatched subgroup");
        let child_type = |context: &RimeraContext, group: RValue| {
            let Some(HeapObject::Exception(group)) = context.heap.get(group) else {
                panic!("expected exception group")
            };
            let Some(HeapObject::Tuple(children)) = group
                .group_exceptions
                .and_then(|value| context.heap.get(value))
            else {
                panic!("expected group children")
            };
            context.exception_type_name(children[0]).unwrap().to_owned()
        };
        assert_eq!(child_type(&context, matched), "ValueError");
        assert_eq!(child_type(&context, rest), "TypeError");

        let (wrapped, rest) = context.split_exception(value_error, expected).unwrap();
        assert!(rest.is_none());
        assert!(matches!(
            context
                .heap
                .get(wrapped.expect("ordinary match is wrapped")),
            Some(HeapObject::Exception(ExceptionObject {
                group_exceptions: Some(_),
                ..
            }))
        ));

        let base_type = context.builtin_type("BaseException").unwrap();
        let base = context.new_exception(base_type, &[]).unwrap();
        let message = context
            .allocate(HeapObject::String("base".to_owned()))
            .unwrap();
        assert!(
            context
                .new_exception_group(group_type, message, &[base])
                .is_err()
        );
        let base_group_type = context.builtin_type("BaseExceptionGroup").unwrap();
        let message = context
            .allocate(HeapObject::String("auto".to_owned()))
            .unwrap();
        let auto = context
            .new_exception_group(base_group_type, message, &[value_error])
            .unwrap();
        assert_eq!(context.exception_type_name(auto), Some("ExceptionGroup"));
    }

    #[test]
    fn builtin_value_families_trace_and_preserve_native_semantics() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let bytes = operations::bytes(&mut context, b"abc").unwrap();
        let bytearray = operations::bytearray(&mut context, b"abc").unwrap();
        let complex = operations::complex(&mut context, 1.0, 2.0).unwrap();
        let slice = operations::slice(
            &mut context,
            Some(RValue::small_int(1)),
            Some(RValue::small_int(3)),
            None,
        )
        .unwrap();
        assert_eq!(
            operations::length(&mut context, bytes).unwrap(),
            RValue::small_int(3)
        );
        assert_eq!(
            operations::item_get(&mut context, bytes, RValue::small_int(-1)).unwrap(),
            RValue::small_int(99)
        );
        let sliced = operations::item_get(&mut context, bytes, slice).unwrap();
        assert_eq!(operations::display(&context, sliced).unwrap(), "b'bc'");
        operations::item_set(
            &mut context,
            bytearray,
            RValue::small_int(0),
            RValue::small_int(122),
        )
        .unwrap();
        assert_eq!(
            operations::display(&context, bytearray).unwrap(),
            "bytearray(b'zbc')"
        );
        let sum = operations::binary(&mut context, 0, complex, RValue::small_int(3)).unwrap();
        assert_eq!(operations::display(&context, sum).unwrap(), "(4+2j)");
        let frozen = operations::frozenset(&mut context, &[bytes]).unwrap();
        assert!(operations::hash(&mut context, frozen).is_ok());
        let view = operations::memoryview(&mut context, bytearray).unwrap();
        assert_eq!(
            operations::item_get(&mut context, view, RValue::small_int(1)).unwrap(),
            RValue::small_int(98)
        );
        let copied = operations::memoryview_to_bytes(&mut context, view).unwrap();
        assert_eq!(operations::display(&context, copied).unwrap(), "b'zbc'");
        let hex = operations::memoryview_hex(&mut context, view, None, 1).unwrap();
        assert_eq!(operations::display(&context, hex).unwrap(), "7a6263");
        let mapping = operations::dictionary(
            &mut context,
            &[RValue::small_int(1), RValue::small_int(2)],
            &[bytes, bytearray],
        )
        .unwrap();
        let keys = operations::dictionary_view(
            &mut context,
            mapping,
            crate::object::DictionaryViewKind::Keys,
        )
        .unwrap();
        assert_eq!(
            operations::length(&mut context, keys).unwrap(),
            RValue::small_int(2)
        );
        let iterator = operations::iterator_new(&mut context, keys).unwrap();
        assert_eq!(
            operations::iterator_next(&mut context, iterator).unwrap(),
            Some(RValue::small_int(1))
        );
        operations::memoryview_release(&mut context, view).unwrap();
        assert!(operations::item_get(&mut context, view, RValue::small_int(0)).is_err());
        let transient_view = operations::memoryview(&mut context, bytearray).unwrap();
        assert!(matches!(
            context.heap.get(transient_view),
            Some(HeapObject::MemoryView(_))
        ));
        context.with_temporary_roots(&[bytearray], |context| context.collect());
        assert!(context.heap.get(transient_view).is_none());
        assert_eq!(
            match context.heap.get(bytearray) {
                Some(HeapObject::ByteArray(value)) => value.exports,
                _ => unreachable!("bytearray remains rooted"),
            },
            0
        );
        context.collect();
        assert!(context.heap.get(bytes).is_none());
    }

    #[test]
    fn gate7_buffer_release_callback_failure_is_unraisable_and_drops_export_once() {
        let _guard = BUFFER_TEST_LOCK.lock().unwrap();
        BUFFER_ACQUIRE_COUNT.store(0, AtomicOrdering::SeqCst);
        BUFFER_RELEASE_COUNT.store(0, AtomicOrdering::SeqCst);
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let (provider, data, inner) = make_buffer_provider(
            &mut context,
            buffer_provider_native as usize,
            raising_buffer_release_native as usize,
        );
        let view = context
            .with_temporary_roots(&[provider, data, inner], |context| {
                operations::memoryview(context, provider)
            })
            .unwrap();
        assert_eq!(BUFFER_ACQUIRE_COUNT.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(
            match context.heap.get(data) {
                Some(HeapObject::ByteArray(data)) => data.exports,
                _ => panic!("provider backing bytearray disappeared"),
            },
            1
        );

        context.raised = None;
        context.exception = None;
        operations::memoryview_release(&mut context, view).unwrap();
        assert_eq!(BUFFER_RELEASE_COUNT.load(AtomicOrdering::SeqCst), 1);
        assert!(context.raised.is_none());
        assert!(context.exception.is_none());
        assert!(matches!(
            context.heap.get(inner),
            Some(HeapObject::MemoryView(inner)) if inner.released
        ));
        assert_eq!(
            match context.heap.get(data) {
                Some(HeapObject::ByteArray(data)) => data.exports,
                _ => panic!("provider backing bytearray disappeared"),
            },
            0
        );
        operations::memoryview_release(&mut context, view).unwrap();
        assert_eq!(BUFFER_RELEASE_COUNT.load(AtomicOrdering::SeqCst), 1);
    }

    #[test]
    fn gate7_buffer_failed_construction_releases_acquired_export_once_under_low_heap() {
        let _guard = BUFFER_TEST_LOCK.lock().unwrap();
        BUFFER_ACQUIRE_COUNT.store(0, AtomicOrdering::SeqCst);
        BUFFER_RELEASE_COUNT.store(0, AtomicOrdering::SeqCst);
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let (provider, data, inner) = make_buffer_provider(
            &mut context,
            constrained_buffer_provider_native as usize,
            counting_buffer_release_native as usize,
        );
        let error = context
            .with_temporary_roots(&[provider, data, inner], |context| {
                operations::memoryview(context, provider)
            })
            .unwrap_err();
        assert_eq!(error, "managed heap limit exceeded");
        assert_eq!(BUFFER_ACQUIRE_COUNT.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(BUFFER_RELEASE_COUNT.load(AtomicOrdering::SeqCst), 1);
        assert!(matches!(
            context.heap.get(inner),
            Some(HeapObject::MemoryView(inner)) if inner.released
        ));
        assert_eq!(
            match context.heap.get(data) {
                Some(HeapObject::ByteArray(data)) => data.exports,
                _ => panic!("provider backing bytearray disappeared"),
            },
            0
        );
        context.set_heap_limit(None).unwrap();
    }

    #[test]
    fn gate10_slice5_async_provider_lease_roots_graph_and_releases_exactly_once() {
        let _guard = BUFFER_TEST_LOCK.lock().unwrap();
        BUFFER_ACQUIRE_COUNT.store(0, AtomicOrdering::SeqCst);
        BUFFER_RELEASE_COUNT.store(0, AtomicOrdering::SeqCst);
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let coroutine_function = test_function(
            &mut context,
            two_stage_generator_resume as usize,
            FunctionKind::Coroutine {
                persistent_slot_count: 0,
            },
            "root",
            "root",
            vec![],
            &[],
        );
        let coroutine = context.new_coroutine(coroutine_function, &[]).unwrap();
        let (provider, data, inner) = make_buffer_provider(
            &mut context,
            buffer_provider_native as usize,
            counting_buffer_release_native as usize,
        );

        // SAFETY: the driver below owns the unique mutable context borrow for
        // its lifetime. This raw pointer is test-only observation of that same
        // local context; it never escapes or aliases another active Rust borrow.
        let context_pointer = std::ptr::from_mut(&mut context);
        let driver = AsyncRuntimeDriver::with_capacity(&mut context, 1);
        let mut task = driver.submit_root(coroutine).unwrap();
        let waker = Waker::noop();
        let mut poll_context = Context::from_waker(waker);
        assert!(matches!(
            Pin::new(&mut task).poll(&mut poll_context),
            Poll::Pending
        ));
        let async_view = task.lease_buffer(provider).unwrap();
        assert_eq!(BUFFER_ACQUIRE_COUNT.load(AtomicOrdering::SeqCst), 1);
        let provider_lease = match unsafe { (*context_pointer).heap.get(async_view) } {
            Some(HeapObject::MemoryView(view)) => view.lease.expect("provider lease"),
            _ => panic!("async lease is not a memoryview"),
        };
        unsafe { (*context_pointer).collect() };
        assert!(unsafe { (*context_pointer).heap.get(provider).is_some() });
        assert!(unsafe { (*context_pointer).heap.get(data).is_some() });
        assert!(unsafe { (*context_pointer).heap.get(inner).is_some() });
        assert!(matches!(
            unsafe { (*context_pointer).heap.get(provider_lease) },
            Some(HeapObject::BufferLease(_))
        ));
        assert_eq!(BUFFER_RELEASE_COUNT.load(AtomicOrdering::SeqCst), 0);
        assert_eq!(
            match unsafe { (*context_pointer).heap.get(data) } {
                Some(HeapObject::ByteArray(data)) => data.exports,
                _ => panic!("provider backing bytearray disappeared"),
            },
            1
        );

        drop(task);
        assert_eq!(BUFFER_RELEASE_COUNT.load(AtomicOrdering::SeqCst), 1);
        assert!(matches!(
            unsafe { (*context_pointer).heap.get(inner) },
            Some(HeapObject::MemoryView(inner)) if inner.released
        ));
        assert_eq!(
            match unsafe { (*context_pointer).heap.get(data) } {
                Some(HeapObject::ByteArray(data)) => data.exports,
                _ => panic!("provider backing bytearray disappeared before release proof"),
            },
            0
        );
        drop(driver);
        context.collect();
        context.collect();
        assert_eq!(BUFFER_RELEASE_COUNT.load(AtomicOrdering::SeqCst), 1);
    }

    #[test]
    fn gate7_buffer_exporter_cycle_finalizes_once_and_collects() {
        let _guard = BUFFER_TEST_LOCK.lock().unwrap();
        BUFFER_ACQUIRE_COUNT.store(0, AtomicOrdering::SeqCst);
        BUFFER_RELEASE_COUNT.store(0, AtomicOrdering::SeqCst);
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let (provider, data, inner) = make_buffer_provider(
            &mut context,
            buffer_provider_native as usize,
            counting_buffer_release_native as usize,
        );
        let view = context
            .with_temporary_roots(&[provider, data, inner], |context| {
                operations::memoryview(context, provider)
            })
            .unwrap();
        context.attribute_set(provider, "cycle", view).unwrap();
        assert_eq!(BUFFER_ACQUIRE_COUNT.load(AtomicOrdering::SeqCst), 1);

        context.collect();
        assert_eq!(BUFFER_RELEASE_COUNT.load(AtomicOrdering::SeqCst), 1);
        assert!(context.heap.get(provider).is_none());
        assert!(context.heap.get(view).is_none());
        assert!(context.heap.get(inner).is_none());
        assert!(context.heap.get(data).is_none());
        context.collect();
        assert_eq!(BUFFER_RELEASE_COUNT.load(AtomicOrdering::SeqCst), 1);
    }

    #[test]
    fn gate4_boolean_truth_callback_survives_forced_collection() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let namespace = context.namespace_new().unwrap();
        let method = test_function(
            &mut context,
            collecting_truth_native as usize,
            FunctionKind::Normal,
            "__bool__",
            "CollectingTruth.__bool__",
            vec![Parameter {
                name: "self".to_owned(),
                kind: ParameterKind::PositionalOrKeyword,
                has_default: false,
            }],
            &[],
        );
        context
            .namespace_set(namespace, "__bool__", method)
            .unwrap();
        let truth_type = context
            .new_class("CollectingTruth", &[], namespace)
            .unwrap();
        let value = context.new_instance(truth_type).unwrap();
        let truth = context
            .with_temporary_roots(&[value], |context| operations::truthy(context, value))
            .unwrap();
        assert!(truth);
        assert!(matches!(
            context.heap.get(value),
            Some(HeapObject::Instance(_))
        ));
    }

    #[test]
    fn gate4_comparison_callback_keeps_both_operands_alive_during_forced_collection() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let namespace = context.namespace_new().unwrap();
        let method = test_function(
            &mut context,
            collecting_less_native as usize,
            FunctionKind::Normal,
            "__lt__",
            "CollectingCompare.__lt__",
            vec![
                Parameter {
                    name: "self".to_owned(),
                    kind: ParameterKind::PositionalOrKeyword,
                    has_default: false,
                },
                Parameter {
                    name: "other".to_owned(),
                    kind: ParameterKind::PositionalOrKeyword,
                    has_default: false,
                },
            ],
            &[],
        );
        context.namespace_set(namespace, "__lt__", method).unwrap();
        let compare_type = context
            .new_class("CollectingCompare", &[], namespace)
            .unwrap();
        let left = context.new_instance(compare_type).unwrap();
        let right = context
            .with_temporary_roots(&[left], |context| context.new_instance(compare_type))
            .unwrap();
        let result = context
            .with_temporary_roots(&[left, right], |context| {
                operations::compare(context, RCompareOperator::Less as u8, left, right)
            })
            .unwrap();
        assert_eq!(result, RValue::boolean(true));
        assert!(matches!(
            context.heap.get(left),
            Some(HeapObject::Instance(_))
        ));
        assert!(matches!(
            context.heap.get(right),
            Some(HeapObject::Instance(_))
        ));
    }

    #[test]
    fn gate4_extended_subscript_callback_keeps_index_tuple_slice_and_rhs_alive() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let namespace = context.namespace_new().unwrap();
        let method = test_function(
            &mut context,
            collecting_setitem_native as usize,
            FunctionKind::Normal,
            "__setitem__",
            "CollectingSubscript.__setitem__",
            vec![
                Parameter {
                    name: "self".to_owned(),
                    kind: ParameterKind::PositionalOrKeyword,
                    has_default: false,
                },
                Parameter {
                    name: "key".to_owned(),
                    kind: ParameterKind::PositionalOrKeyword,
                    has_default: false,
                },
                Parameter {
                    name: "value".to_owned(),
                    kind: ParameterKind::PositionalOrKeyword,
                    has_default: false,
                },
            ],
            &[],
        );
        context
            .namespace_set(namespace, "__setitem__", method)
            .unwrap();
        let subscript_type = context
            .new_class("CollectingSubscript", &[], namespace)
            .unwrap();
        let receiver = context.new_instance(subscript_type).unwrap();
        let slice = context
            .with_temporary_roots(&[receiver], |context| {
                operations::slice(
                    context,
                    Some(RValue::small_int(1)),
                    Some(RValue::small_int(5)),
                    Some(RValue::small_int(2)),
                )
            })
            .unwrap();
        let index = context
            .with_temporary_roots(&[receiver, slice], |context| {
                operations::tuple(context, &[RValue::small_int(3), slice])
            })
            .unwrap();
        let rhs = context
            .with_temporary_roots(&[receiver, index], |context| {
                operations::string(context, "rhs")
            })
            .unwrap();
        context
            .with_temporary_roots(&[receiver, index, rhs], |context| {
                operations::item_set(context, receiver, index, rhs)
            })
            .unwrap();
        assert!(matches!(
            context.heap.get(index),
            Some(HeapObject::Tuple(_))
        ));
        assert!(matches!(
            context.heap.get(slice),
            Some(HeapObject::Slice(_))
        ));
        assert_eq!(operations::display(&context, rhs).unwrap(), "rhs");
    }

    #[test]
    fn gate4_dictionary_merge_roots_values_across_hash_and_equality_callbacks() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let namespace = context.namespace_new().unwrap();
        let hash_function = test_function(
            &mut context,
            collecting_hash_native as usize,
            FunctionKind::Normal,
            "__hash__",
            "CollectingKey.__hash__",
            vec![Parameter {
                name: "self".to_owned(),
                kind: ParameterKind::PositionalOrKeyword,
                has_default: false,
            }],
            &[],
        );
        let equal_function = context.with_temporary_roots(&[namespace, hash_function], |context| {
            test_function(
                context,
                collecting_equal_native as usize,
                FunctionKind::Normal,
                "__eq__",
                "CollectingKey.__eq__",
                vec![
                    Parameter {
                        name: "self".to_owned(),
                        kind: ParameterKind::PositionalOrKeyword,
                        has_default: false,
                    },
                    Parameter {
                        name: "other".to_owned(),
                        kind: ParameterKind::PositionalOrKeyword,
                        has_default: false,
                    },
                ],
                &[],
            )
        });
        context
            .namespace_set(namespace, "__hash__", hash_function)
            .unwrap();
        context
            .namespace_set(namespace, "__eq__", equal_function)
            .unwrap();
        let key_type = context.new_class("CollectingKey", &[], namespace).unwrap();
        let first = context.new_instance(key_type).unwrap();
        let first_value = operations::string(&mut context, "first").unwrap();
        let target = operations::dictionary(&mut context, &[first], &[first_value]).unwrap();

        let (second, second_value, source) = context
            .with_temporary_roots(&[target], |context| {
                let second = context.new_instance(key_type)?;
                let second_value = operations::string(context, "replacement")?;
                let source = context
                    .with_temporary_roots(&[target, second, second_value], |context| {
                        operations::dictionary(context, &[second], &[second_value])
                    })?;
                Ok::<_, String>((second, second_value, source))
            })
            .unwrap();

        context
            .with_temporary_roots(&[target, source], |context| {
                call::dict_merge_mapping_source(context, target, source)
            })
            .unwrap();
        context.with_temporary_roots(&[target], |context| context.collect());
        let (stored_key, stored_value) = match context.heap.get(target) {
            Some(HeapObject::ValueDictionary(dictionary)) => dictionary
                .table
                .values()
                .next()
                .copied()
                .expect("merged dictionary keeps one entry"),
            _ => panic!("merge target stopped being a native dictionary"),
        };
        assert_eq!(
            stored_key, first,
            "equal replacement must retain the first key"
        );
        assert_eq!(stored_value, second_value);
        assert_eq!(
            operations::display(&context, second_value).unwrap(),
            "replacement"
        );
        assert!(context.heap.get(second).is_none());
        assert!(context.heap.get(first).is_some());
    }

    #[test]
    fn gate4_prepared_call_arguments_trace_callable_and_accumulated_values() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let callable = test_function(
            &mut context,
            add_native as usize,
            FunctionKind::Normal,
            "prepared",
            "prepared",
            Vec::new(),
            &[],
        );
        let arguments = call::call_arguments_new(&mut context, callable).unwrap();
        let first = operations::string(&mut context, "first").unwrap();
        call::call_argument_add(
            &mut context,
            arguments,
            RCallArgumentKind::Positional,
            None,
            first,
        )
        .unwrap();
        let second = operations::string(&mut context, "second").unwrap();
        let source = context
            .with_temporary_roots(&[arguments, second], |context| {
                operations::list(context, &[second])
            })
            .unwrap();
        context
            .with_temporary_roots(&[arguments, source], |context| {
                call::call_argument_add(
                    context,
                    arguments,
                    RCallArgumentKind::Starred,
                    None,
                    source,
                )
            })
            .unwrap();
        call::call_argument_add(
            &mut context,
            arguments,
            RCallArgumentKind::Keyword,
            Some("named"),
            second,
        )
        .unwrap();

        context.with_temporary_roots(&[arguments], |context| context.collect());
        let Some(HeapObject::CallArguments(prepared)) = context.heap.get(arguments) else {
            panic!("prepared call accumulator was collected");
        };
        assert_eq!(prepared.callable, callable);
        assert_eq!(prepared.positional, vec![first, second]);
        assert_eq!(prepared.keywords, vec![("named".to_owned(), second)]);
        assert_eq!(operations::display(&context, first).unwrap(), "first");
        assert_eq!(operations::display(&context, second).unwrap(), "second");
        assert!(context.heap.get(source).is_none());
        assert!(context.heap.get(callable).is_some());

        context.collect();
        assert!(context.heap.get(arguments).is_none());
        assert!(context.heap.get(callable).is_none());
        assert!(context.heap.get(first).is_none());
        assert!(context.heap.get(second).is_none());
    }

    #[test]
    fn gate4_prepared_call_argument_growth_refreshes_managed_bytes_and_heap_limit() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let callable = test_function(
            &mut context,
            add_native as usize,
            FunctionKind::Normal,
            "prepared",
            "prepared",
            Vec::new(),
            &[],
        );
        let mut arguments = RValue::NONE;
        assert_eq!(
            // SAFETY: callable/output storage and context remain live for the call.
            unsafe {
                rimera_call_arguments_new(&raw mut context, &raw const callable, &raw mut arguments)
            },
            RStatus::Ok
        );
        let before = context.heap.live_bytes();
        context
            .set_heap_limit(Some(before.saturating_add(1)))
            .unwrap();
        let value = RValue::small_int(7);
        let status = context.with_temporary_roots(&[callable, arguments], |context| {
            // SAFETY: all input storage and the context remain live for the call.
            unsafe {
                rimera_call_argument_add(
                    &raw mut *context,
                    &raw const arguments,
                    RCallArgumentKind::Positional as u8,
                    std::ptr::null(),
                    0,
                    &raw const value,
                )
            }
        });
        assert_eq!(status, RStatus::Exception);
        assert!(context.heap.live_bytes() > before);
        let raised = context
            .raised
            .expect("retained accumulator growth should raise MemoryError");
        assert_eq!(
            context.type_of(raised).unwrap(),
            context.builtin_type("MemoryError").unwrap()
        );
        let Some(HeapObject::CallArguments(prepared)) = context.heap.get(arguments) else {
            panic!("rooted prepared-call accumulator was collected");
        };
        assert_eq!(prepared.positional, vec![value]);
    }

    #[test]
    fn gate4_comprehension_list_sink_growth_refreshes_managed_bytes_and_heap_limit() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let list = operations::list(&mut context, &[]).unwrap();
        context.add_context_root(list);
        let before = context.heap.live_bytes();
        context
            .set_heap_limit(Some(before.saturating_add(1)))
            .unwrap();
        let value = RValue::small_int(7);
        let status =
            unsafe { rimera_list_append(&raw mut context, &raw const list, &raw const value) };
        assert_eq!(status, RStatus::Exception);
        assert!(context.heap.live_bytes() > before);
        assert_eq!(operations::display(&context, list).unwrap(), "[7]");
        let raised = context
            .raised
            .expect("retained comprehension list growth should raise MemoryError");
        assert_eq!(
            context.type_of(raised).unwrap(),
            context.builtin_type("MemoryError").unwrap()
        );
    }

    #[test]
    fn gate4_comprehension_hash_sinks_preserve_failures_without_partial_entries() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let set = operations::set(&mut context, &[]).unwrap();
        let dictionary = operations::dictionary(&mut context, &[], &[]).unwrap();
        let bad_key = operations::list(&mut context, &[RValue::small_int(1)]).unwrap();
        context.add_context_root(set);
        context.add_context_root(dictionary);
        context.add_context_root(bad_key);

        let set_status =
            unsafe { rimera_set_insert(&raw mut context, &raw const set, &raw const bad_key) };
        assert_eq!(set_status, RStatus::Exception);
        assert_eq!(operations::display(&context, set).unwrap(), "set()");
        let set_error = context
            .raised
            .take()
            .expect("set insert should raise TypeError");
        assert_eq!(
            context.type_of(set_error).unwrap(),
            context.builtin_type("TypeError").unwrap()
        );

        let value = RValue::small_int(9);
        let dict_status = unsafe {
            rimera_dictionary_insert(
                &raw mut context,
                &raw const dictionary,
                &raw const bad_key,
                &raw const value,
            )
        };
        assert_eq!(dict_status, RStatus::Exception);
        assert_eq!(operations::display(&context, dictionary).unwrap(), "{}");
        let dict_error = context
            .raised
            .expect("dict insert should preserve the hashing TypeError");
        assert_eq!(
            context.type_of(dict_error).unwrap(),
            context.builtin_type("TypeError").unwrap()
        );
    }

    #[test]
    fn gate4_comprehension_dict_insert_survives_gc_and_unrelated_table_mutation_in_hash() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let unrelated = operations::dictionary(&mut context, &[], &[]).unwrap();
        context.add_context_root(unrelated);
        let namespace = context.namespace_new().unwrap();
        let hash_function = test_function(
            &mut context,
            collecting_mutating_hash_native as usize,
            FunctionKind::Normal,
            "__hash__",
            "ComprehensionKey.__hash__",
            vec![Parameter {
                name: "self".to_owned(),
                kind: ParameterKind::PositionalOrKeyword,
                has_default: false,
            }],
            &[unrelated],
        );
        context
            .namespace_set(namespace, "__hash__", hash_function)
            .unwrap();
        let key_type = context
            .new_class("ComprehensionKey", &[], namespace)
            .unwrap();
        let key = context.new_instance(key_type).unwrap();
        let value = operations::string(&mut context, "kept").unwrap();
        let dictionary = operations::dictionary(&mut context, &[], &[]).unwrap();

        let status =
            context.with_temporary_roots(&[dictionary, key, value, unrelated], |context| unsafe {
                rimera_dictionary_insert(
                    &raw mut *context,
                    &raw const dictionary,
                    &raw const key,
                    &raw const value,
                )
            });
        assert_eq!(status, RStatus::Ok);
        context.with_temporary_roots(&[dictionary, unrelated], |context| context.collect());

        assert_eq!(
            operations::item_get(&mut context, unrelated, RValue::small_int(1)).unwrap(),
            RValue::small_int(99)
        );
        let (stored_key, stored_value) = match context.heap.get(dictionary) {
            Some(HeapObject::ValueDictionary(dictionary)) => dictionary
                .table
                .values()
                .next()
                .copied()
                .expect("comprehension dictionary should contain one entry"),
            _ => panic!("comprehension sink should retain a value dictionary"),
        };
        assert_eq!(stored_key, key);
        assert_eq!(operations::display(&context, stored_value).unwrap(), "kept");
    }

    #[test]
    fn gate4_extended_unpack_abi_preserves_prefix_star_suffix_and_value_errors() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let source = operations::list(
            &mut context,
            &[
                RValue::small_int(1),
                RValue::small_int(2),
                RValue::small_int(3),
                RValue::small_int(4),
            ],
        )
        .unwrap();
        let mut result = RValue::NONE;
        assert_eq!(
            // SAFETY: source/output storage and context remain live for the call.
            unsafe {
                rimera_unpack_ex(
                    &raw mut context,
                    &raw const source,
                    1,
                    1,
                    1,
                    &raw mut result,
                )
            },
            RStatus::Ok
        );
        assert_eq!(
            operations::value_array_get(&context, result, 0).unwrap(),
            RValue::small_int(1)
        );
        let star = operations::value_array_get(&context, result, 1).unwrap();
        assert_eq!(operations::display(&context, star).unwrap(), "[2, 3]");
        assert_eq!(
            operations::value_array_get(&context, result, 2).unwrap(),
            RValue::small_int(4)
        );

        context.raised = None;
        let short = operations::list(&mut context, &[RValue::small_int(9)]).unwrap();
        let mut ignored = RValue::NONE;
        assert_eq!(
            // SAFETY: source/output storage and context remain live for the call.
            unsafe {
                rimera_unpack_ex(
                    &raw mut context,
                    &raw const short,
                    1,
                    1,
                    1,
                    &raw mut ignored,
                )
            },
            RStatus::Exception
        );
        let raised = context
            .raised
            .expect("extended unpack should raise ValueError");
        let value_error = context.builtin_type("ValueError").unwrap();
        assert_eq!(
            context.type_of(raised).unwrap(),
            value_error,
            "too-few extended unpack must not be reclassified by the ABI"
        );
        assert_eq!(
            operations::display(&context, raised).unwrap(),
            "not enough values to unpack (expected at least 2, got 1)"
        );
    }

    #[test]
    fn gate4_starred_unpack_low_heap_roots_source_until_memory_error() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let values = [
            operations::string(&mut context, "alpha").unwrap(),
            operations::string(&mut context, "beta").unwrap(),
            operations::string(&mut context, "gamma").unwrap(),
        ];
        let source = context
            .with_temporary_roots(&values, |context| operations::list(context, &values))
            .unwrap();

        let without_iterator = context.heap.live_bytes();
        let probe = context
            .with_temporary_roots(&[source], |context| {
                operations::iterator_new(context, source)
            })
            .unwrap();
        let iterator_bytes = context.heap.live_bytes().saturating_sub(without_iterator);
        assert!(iterator_bytes > 0);
        context.with_temporary_roots(&[source], |context| context.collect());
        assert!(context.heap.get(probe).is_none());

        let rooted_source_bytes = context.heap.live_bytes();
        let limit = rooted_source_bytes.saturating_add(iterator_bytes);
        context
            .with_temporary_roots(&[source], |context| context.set_heap_limit(Some(limit)))
            .unwrap();
        let error = operations::unpack(&mut context, source, 0, 0, true).unwrap_err();
        assert_eq!(error, "managed heap limit exceeded");
        assert!(
            context.heap.get(source).is_some(),
            "source iterable must remain rooted across the failing star-list allocation"
        );
        assert_eq!(
            operations::display(&context, source).unwrap(),
            "['alpha', 'beta', 'gamma']"
        );
    }

    #[test]
    fn memoryview_casts_native_scalar_formats_and_releases_exports() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let backing = operations::bytearray(&mut context, &[1, 0, 2, 0]).unwrap();
        let view = operations::memoryview(&mut context, backing).unwrap();
        let cast = operations::memoryview_cast(&mut context, view, "H", None).unwrap();
        assert_eq!(
            operations::item_get(&mut context, cast, RValue::small_int(0)).unwrap(),
            RValue::small_int(1)
        );
        assert_eq!(
            operations::item_get(&mut context, cast, RValue::small_int(1)).unwrap(),
            RValue::small_int(2)
        );
        let values = operations::memoryview_to_list(&mut context, cast).unwrap();
        assert_eq!(operations::display(&context, values).unwrap(), "[1, 2]");
        operations::memoryview_release(&mut context, cast).unwrap();
        operations::memoryview_release(&mut context, view).unwrap();
        operations::memoryview_release(&mut context, view).unwrap();
        assert_eq!(
            match context.heap.get(backing) {
                Some(HeapObject::ByteArray(bytes)) => bytes.exports,
                _ => unreachable!("backing value is a bytearray"),
            },
            0
        );
    }
}
