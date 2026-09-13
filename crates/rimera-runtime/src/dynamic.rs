use std::fmt;

use num_traits::ToPrimitive;
use rimera_abi::{RDynamicCompileMode, RNativeFunction, RStatus, RValue};

use crate::context::RimeraContext;
use crate::heap::HeapObject;
use crate::object::{CodeObject, FunctionKind, FunctionObject};
use crate::operations;

pub const MAX_DYNAMIC_SOURCE_BYTES: usize = 1024 * 1024;
pub const MAX_DYNAMIC_NATIVE_UNITS: usize = 128;
pub const MAX_DYNAMIC_EXECUTION_DEPTH: usize = 32;

pub struct NativeDynamicCode {
    pub address: usize,
    pub source: String,
    pub mode: RDynamicCompileMode,
    pub filename: String,
    pub flags: u32,
    pub optimize: i32,
    pub owner: Box<dyn fmt::Debug>,
}

impl fmt::Debug for NativeDynamicCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeDynamicCode")
            .field("filename", &self.filename)
            .field("mode", &self.mode)
            .field("flags", &self.flags)
            .field("optimize", &self.optimize)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct DynamicCompileError {
    pub exception_type: &'static str,
    pub message: String,
    pub offset: usize,
}

pub type DynamicCompiler =
    fn(&str, &str, RDynamicCompileMode, u32, i32) -> Result<NativeDynamicCode, DynamicCompileError>;

#[derive(Debug, Default)]
pub(crate) struct DynamicState {
    pub compiler: Option<DynamicCompiler>,
    pub native_units: Vec<NativeDynamicCode>,
    pub publishing_address: Option<usize>,
    pub function_builtins: Vec<(RValue, RValue)>,
    pub execution_depth: usize,
}

impl RimeraContext {
    pub fn install_dynamic_compiler(&mut self, compiler: DynamicCompiler) {
        self.enable_dynamic_compilation();
        self.dynamic.compiler = Some(compiler);
    }

    pub(crate) fn compile_dynamic(
        &mut self,
        source: &str,
        filename: &str,
        mode: RDynamicCompileMode,
        flags: u32,
        optimize: i32,
    ) -> Result<RValue, String> {
        if source.len() > MAX_DYNAMIC_SOURCE_BYTES {
            return self.raise_error(
                "RuntimeError",
                format!(
                    "dynamic source exceeds the {} byte limit",
                    MAX_DYNAMIC_SOURCE_BYTES
                ),
            );
        }
        let Some(compiler) = self.dynamic.compiler else {
            return self.raise_error(
                "RuntimeError",
                "dynamic compiler service is not linked for this artifact",
            );
        };
        let cached = self
            .dynamic
            .native_units
            .iter()
            .find(|unit| {
                unit.source == source
                    && unit.filename == filename
                    && unit.mode == mode
                    && unit.flags == flags
                    && unit.optimize == optimize
            })
            .map(|unit| (unit.address, unit.flags));
        let cached_address = cached.map(|(address, _)| address);
        let cached_flags = cached.map(|(_, flags)| flags);
        if cached.is_none() && self.dynamic.native_units.len() >= MAX_DYNAMIC_NATIVE_UNITS {
            self.collect();
            if self.dynamic.native_units.len() >= MAX_DYNAMIC_NATIVE_UNITS {
                return self.raise_error(
                    "RuntimeError",
                    format!(
                        "dynamic native unit limit of {} reached",
                        MAX_DYNAMIC_NATIVE_UNITS
                    ),
                );
            }
        }
        let native = if cached.is_some() {
            None
        } else {
            Some(match compiler(source, filename, mode, flags, optimize) {
                Ok(native) => native,
                Err(error) => {
                    self.raise_builtin(error.exception_type, error.message.clone())?;
                    if error.exception_type == "SyntaxError" {
                        let exception = self.raised.ok_or("syntax exception unavailable")?;
                        self.with_temporary_roots(&[exception], |context| {
                            let offset = error.offset.min(source.len());
                            let line = source[..offset]
                                .bytes()
                                .filter(|byte| *byte == b'\n')
                                .count()
                                + 1;
                            let line_start =
                                source[..offset].rfind('\n').map_or(0, |index| index + 1);
                            let line_end = source[offset..]
                                .find('\n')
                                .map_or(source.len(), |index| offset + index + 1);
                            for (name, text) in [
                                ("filename", filename),
                                ("text", &source[line_start..line_end]),
                                ("msg", &error.message),
                            ] {
                                let value = operations::string(context, text)?;
                                context.attribute_set(exception, name, value)?;
                            }
                            context.attribute_set(
                                exception,
                                "lineno",
                                RValue::small_int(line as i64),
                            )?;
                            context.attribute_set(
                                exception,
                                "offset",
                                RValue::small_int(
                                    source[line_start..offset].chars().count() as i64 + 1,
                                ),
                            )?;
                            context.attribute_set(exception, "end_lineno", RValue::NONE)?;
                            context.attribute_set(exception, "end_offset", RValue::NONE)
                        })?;
                    }
                    return Err(error.message);
                }
            })
        };
        let address = cached_address.unwrap_or_else(|| native.as_ref().unwrap().address);
        let published_flags = cached_flags.unwrap_or_else(|| native.as_ref().unwrap().flags);
        let code = CodeObject {
            dynamic_mode: Some(mode),
            flags_override: Some(published_flags),
            native_unit_address: Some(address),
            code_address: address,
            kind: FunctionKind::Normal,
            name: "<module>".to_owned(),
            qualified_name: "<module>".to_owned(),
            parameters: Box::new([]),
            filename: filename.to_owned(),
            first_line: 1,
            local_names: Box::new([]),
            cell_names: Box::new([]),
            free_names: Box::new([]),
        };
        self.dynamic.publishing_address = Some(address);
        let value = self.allocate(HeapObject::Code(code));
        self.dynamic.publishing_address = None;
        let value = value?;
        if let Some(native) = native {
            self.dynamic.native_units.push(native);
        }
        Ok(value)
    }

    pub(crate) fn execute_dynamic(
        &mut self,
        code_value: RValue,
        globals: RValue,
        locals: RValue,
        closure: Option<RValue>,
        eval: bool,
    ) -> Result<RValue, String> {
        let Some(HeapObject::Code(code)) = self.heap.get(code_value) else {
            return self.raise_error("TypeError", "expected a code object");
        };
        let code = code.clone();
        if eval && !code.free_names.is_empty() {
            return self.raise_error(
                "TypeError",
                "code object passed to eval() may not contain free variables",
            );
        }
        let closure = closure.filter(|value| *value != RValue::NONE);
        if !code.free_names.is_empty() || closure.is_some() {
            let valid = closure.is_some_and(|value| matches!(self.heap.get(value),
                Some(HeapObject::Tuple(cells)) if cells.len() == code.free_names.len()
                && cells.iter().all(|cell| matches!(self.heap.get(*cell), Some(HeapObject::Cell(_))))));
            if code.free_names.is_empty() {
                return self.raise_error("TypeError", "cannot use a closure with this code object");
            }
            if !valid {
                return self.raise_error(
                    "TypeError",
                    format!(
                        "code object requires a closure of exactly length {}",
                        code.free_names.len()
                    ),
                );
            }
        }
        if code.code_address == 0 {
            return self.raise_error("TypeError", "code object has no executable native entry");
        }
        if self.dynamic.execution_depth >= MAX_DYNAMIC_EXECUTION_DEPTH {
            return self.raise_error(
                "RuntimeError",
                format!(
                    "dynamic execution depth exceeds the {} level limit",
                    MAX_DYNAMIC_EXECUTION_DEPTH
                ),
            );
        }
        let mut roots = vec![code_value, globals, locals];
        roots.extend(closure);
        self.dynamic.execution_depth += 1;
        let result = self.with_temporary_roots(&roots, |context| {
            let function = context.allocate(HeapObject::Function(FunctionObject {
                code: code_value,
                fast_call: code.fast_call_metadata(),
                globals,
                name: code.name.clone(),
                qualified_name: code.qualified_name.clone(),
                closure,
                defaults: None,
                keyword_defaults: None,
                annotations: None,
                type_params: None,
            }))?;
            context.capture_function_builtins(function, globals);
            context.with_temporary_roots(&[function], |context| {
                if code.dynamic_mode.is_none() {
                    let output = crate::call::invoke(context, function, &[], &[])?;
                    return Ok(if eval { output } else { RValue::NONE });
                }
                context.push_module_namespace(globals);
                context.push_active_call(function, None);
                if let Err(error) = context.configure_active_scope(Some(locals), false) {
                    context.pop_active_call();
                    context.pop_module_namespace(globals);
                    return Err(error);
                }
                let mut output = RValue::NONE;
                // SAFETY: code publication follows native finalization and its
                // context-owned allocation remains live for this activation.
                let native: RNativeFunction = unsafe { std::mem::transmute(code.code_address) };
                let arguments = if code.dynamic_mode.is_some() {
                    &[locals][..]
                } else {
                    &[][..]
                };
                let status = unsafe {
                    native(
                        std::ptr::from_mut(context).cast(),
                        &function,
                        arguments.as_ptr(),
                        arguments.len(),
                        &mut output,
                    )
                };
                context.pop_active_call();
                context.pop_module_namespace(globals);
                if status == RStatus::Ok {
                    Ok(if eval { output } else { RValue::NONE })
                } else {
                    Err("dynamic code raised an exception".to_owned())
                }
            })
        });
        self.dynamic.execution_depth -= 1;
        result
    }
}

impl RimeraContext {
    pub(crate) fn reclaim_dynamic_state(&mut self) {
        self.dynamic
            .function_builtins
            .retain(|(function, _)| self.heap.get(*function).is_some());
        let publishing = self.dynamic.publishing_address;
        let live_addresses = self
            .heap
            .live_values()
            .into_iter()
            .filter_map(|value| match self.heap.get(value) {
                Some(HeapObject::Code(code)) => code.native_unit_address,
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>();
        self.dynamic.native_units.retain(|unit| {
            Some(unit.address) == publishing || live_addresses.contains(&unit.address)
        });
    }
}

fn source_string(context: &mut RimeraContext, value: RValue) -> Result<String, String> {
    match context.heap.get(value) {
        Some(HeapObject::String(value)) => Ok(value.clone()),
        Some(HeapObject::Bytes(_) | HeapObject::ByteArray(_) | HeapObject::MemoryView(_)) => {
            let bytes = operations::byte_values(context, value)?;
            let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
            match std::str::from_utf8(bytes) {
                Ok(text) => Ok(text.to_owned()),
                Err(_) => context.raise_error("SyntaxError", "source bytes are not valid UTF-8"),
            }
        }
        _ => context.raise_error("TypeError", "source must be a string or bytes-like object"),
    }
}

fn filename_string(context: &mut RimeraContext, value: RValue) -> Result<String, String> {
    match context.heap.get(value) {
        Some(HeapObject::String(value)) => Ok(value.clone()),
        Some(HeapObject::Bytes(_)) => {
            let bytes = operations::byte_values(context, value)?;
            std::str::from_utf8(&bytes)
                .map(str::to_owned)
                .map_err(|_| "compile() filename bytes must be valid UTF-8".to_owned())
        }
        _ => context.raise_error(
            "TypeError",
            "compile() filename must be a string or bytes object",
        ),
    }
}

pub(crate) fn compile(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    let names = [
        "source",
        "filename",
        "mode",
        "flags",
        "dont_inherit",
        "optimize",
    ];
    if positional.len() > names.len() {
        return Err("compile() takes at most 6 arguments".to_owned());
    }
    let mut args = [None; 6];
    for (slot, value) in args.iter_mut().zip(positional) {
        *slot = Some(*value);
    }
    for (name, value) in keywords {
        let Some(index) = names.iter().position(|candidate| candidate == name) else {
            return Err(format!(
                "'{name}' is an invalid keyword argument for compile()"
            ));
        };
        if args[index].replace(*value).is_some() {
            return Err(format!(
                "argument for compile() given by name ('{name}') and position"
            ));
        }
    }
    if args[..3].iter().any(Option::is_none) {
        return Err("compile() missing required argument".to_owned());
    }
    let source = source_string(context, args[0].unwrap())?;
    let filename = filename_string(context, args[1].unwrap())?;
    let mode = operations::string_value(context, args[2].unwrap())
        .ok_or("compile() mode must be a string")?;
    let mode = match mode {
        "exec" => RDynamicCompileMode::Exec,
        "eval" => RDynamicCompileMode::Eval,
        "single" => RDynamicCompileMode::Single,
        _ => {
            return context.raise_error(
                "ValueError",
                "compile() mode must be 'exec', 'eval' or 'single'",
            );
        }
    };
    let flags = match args[3] {
        Some(value) => operations::index_integer(context, value)?
            .to_u32()
            .ok_or("compile(): unrecognised flags")?,
        None => 0,
    };
    if let Some(value) = args[4] {
        // CPython accepts arbitrary objects here and observes truthiness; the
        // value only controls future-flag inheritance, which is inert while
        // Rimera rejects all non-zero compile flags.
        let _ = operations::truthy(context, value)?;
    }
    let optimize = match args[5] {
        Some(value) => operations::index_integer(context, value)?
            .to_i32()
            .ok_or("compile(): invalid optimize value")?,
        None => -1,
    };
    context.compile_dynamic(&source, &filename, mode, flags, optimize)
}

pub(crate) fn execute(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
    eval: bool,
) -> Result<RValue, String> {
    let name = if eval { "eval" } else { "exec" };
    if positional.is_empty() {
        return context.raise_error(
            "TypeError",
            format!("{name} expected at least 1 argument, got 0"),
        );
    }
    if positional.len() > 3 {
        return context.raise_error(
            "TypeError",
            format!(
                "{name} expected at most 3 arguments, got {}",
                positional.len()
            ),
        );
    }
    if keywords.len() > 1 || keywords.iter().any(|(name, _)| eval || name != "closure") {
        return context.raise_error(
            "TypeError",
            if eval {
                "eval() takes no keyword arguments".to_owned()
            } else {
                format!(
                    "'{}' is an invalid keyword argument for exec()",
                    keywords[0].0
                )
            },
        );
    }
    let globals = positional.get(1).copied().filter(|v| *v != RValue::NONE);
    let locals = positional.get(2).copied().filter(|v| *v != RValue::NONE);
    let locals = match locals.or(globals) {
        Some(value) => value,
        None => context.current_locals()?,
    };
    let globals = globals
        .or(context.globals())
        .ok_or("globals are unavailable")?;
    if context.dictionary_storage(globals).is_none() {
        let globals_type = context.type_of(globals)?;
        return context.raise_error(
            "TypeError",
            if eval {
                "globals must be a real dict; try eval(expr, {}, mapping)".to_owned()
            } else {
                format!(
                    "exec() globals must be a dict, not {}",
                    context.type_name(globals_type)
                )
            },
        );
    }
    if context.dictionary_storage(locals).is_none()
        && !context.has_special_method_slot(locals, "__getitem__")?
    {
        return context.raise_error("TypeError", "locals must be a mapping");
    }
    let closure = keywords.first().map(|(_, value)| *value);
    let mut roots = vec![globals, locals];
    roots.extend_from_slice(positional);
    roots.extend(closure);
    context.with_temporary_roots(&roots, |context| {
        if context.namespace_value(globals, "__builtins__").is_none() {
            let builtins = context.execution_builtins();
            let storage = context
                .dictionary_storage(globals)
                .ok_or("globals must be a dict")?;
            context.namespace_set(storage, "__builtins__", builtins)?;
        }
        let code = if matches!(context.heap.get(positional[0]), Some(HeapObject::Code(_))) {
            positional[0]
        } else {
            if closure.is_some_and(|value| value != RValue::NONE) {
                return Err("closure can only be used when source is a code object".to_owned());
            }
            if !matches!(
                context.heap.get(positional[0]),
                Some(
                    HeapObject::String(_)
                        | HeapObject::Bytes(_)
                        | HeapObject::ByteArray(_)
                        | HeapObject::MemoryView(_)
                )
            ) {
                return context.raise_error(
                    "TypeError",
                    format!("{name}() arg 1 must be a string, bytes or code object"),
                );
            }
            let source = source_string(context, positional[0])?;
            let source = if eval {
                source.trim_start_matches([' ', '\t'])
            } else {
                &source
            };
            context.compile_dynamic(
                source,
                "<string>",
                if eval {
                    RDynamicCompileMode::Eval
                } else {
                    RDynamicCompileMode::Exec
                },
                0,
                -1,
            )?
        };
        context.execute_dynamic(code, globals, locals, closure, eval)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COMPILE_CALLS: AtomicUsize = AtomicUsize::new(0);
    static OWNER_DROPS: AtomicUsize = AtomicUsize::new(0);
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[derive(Debug)]
    struct TestOwner;

    impl Drop for TestOwner {
        fn drop(&mut self) {
            OWNER_DROPS.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn test_compiler(
        source: &str,
        filename: &str,
        mode: RDynamicCompileMode,
        flags: u32,
        optimize: i32,
    ) -> Result<NativeDynamicCode, DynamicCompileError> {
        COMPILE_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(NativeDynamicCode {
            address: source.len() + filename.len() + optimize.unsigned_abs() as usize + 1,
            source: source.to_owned(),
            mode,
            filename: filename.to_owned(),
            flags,
            optimize,
            owner: Box::new(TestOwner),
        })
    }

    #[test]
    fn cache_keys_reuse_native_units_and_collection_reclaims_dead_owners() {
        let _guard = TEST_LOCK.lock().unwrap();
        COMPILE_CALLS.store(0, Ordering::SeqCst);
        OWNER_DROPS.store(0, Ordering::SeqCst);
        let mut context = RimeraContext::default();
        context.install_dynamic_compiler(test_compiler);

        let first = context
            .compile_dynamic("40 + 2", "<cache>", RDynamicCompileMode::Eval, 0, -1)
            .unwrap();
        let second = context
            .compile_dynamic("40 + 2", "<cache>", RDynamicCompileMode::Eval, 0, -1)
            .unwrap();
        assert_ne!(
            first, second,
            "compile() must publish distinct code objects"
        );
        assert_eq!(COMPILE_CALLS.load(Ordering::SeqCst), 1);
        assert_eq!(context.dynamic.native_units.len(), 1);

        context.add_context_root(second);
        context.collect();
        assert_eq!(context.dynamic.native_units.len(), 1);
        assert_eq!(OWNER_DROPS.load(Ordering::SeqCst), 0);

        assert!(context.remove_context_root(second));
        context.collect();
        assert!(context.dynamic.native_units.is_empty());
        assert_eq!(OWNER_DROPS.load(Ordering::SeqCst), 1);

        let third = context
            .compile_dynamic("40 + 2", "<cache>", RDynamicCompileMode::Eval, 0, -1)
            .unwrap();
        context.add_context_root(third);
        assert_eq!(COMPILE_CALLS.load(Ordering::SeqCst), 2);
        assert_eq!(context.dynamic.native_units.len(), 1);
    }

    #[test]
    fn source_and_live_native_unit_limits_fail_before_publication() {
        let _guard = TEST_LOCK.lock().unwrap();
        COMPILE_CALLS.store(0, Ordering::SeqCst);
        OWNER_DROPS.store(0, Ordering::SeqCst);
        let mut context = RimeraContext::default();
        context.install_dynamic_compiler(test_compiler);

        let oversized = "x".repeat(MAX_DYNAMIC_SOURCE_BYTES + 1);
        assert!(
            context
                .compile_dynamic(&oversized, "<large>", RDynamicCompileMode::Exec, 0, -1)
                .is_err()
        );
        assert_eq!(COMPILE_CALLS.load(Ordering::SeqCst), 0);
        assert!(context.dynamic.native_units.is_empty());
        assert!(context.consume_exception_type("RuntimeError"));

        for index in 0..MAX_DYNAMIC_NATIVE_UNITS {
            let source = format!("{index}");
            let code = context
                .compile_dynamic(&source, "<units>", RDynamicCompileMode::Eval, 0, -1)
                .unwrap();
            context.add_context_root(code);
        }
        assert_eq!(context.dynamic.native_units.len(), MAX_DYNAMIC_NATIVE_UNITS);
        assert!(
            context
                .compile_dynamic("overflow", "<units>", RDynamicCompileMode::Eval, 0, -1)
                .is_err()
        );
        assert_eq!(context.dynamic.native_units.len(), MAX_DYNAMIC_NATIVE_UNITS);
        assert_eq!(
            COMPILE_CALLS.load(Ordering::SeqCst),
            MAX_DYNAMIC_NATIVE_UNITS
        );
        assert!(context.consume_exception_type("RuntimeError"));
    }

    #[test]
    fn invalid_source_injects_builtins_before_raising_type_error() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let globals = operations::dictionary(&mut context, &[], &[]).unwrap();
        context.add_context_root(globals);
        assert!(execute(&mut context, &[RValue::small_int(42), globals], &[], true).is_err());
        assert!(context.namespace_value(globals, "__builtins__").is_some());
        assert!(context.consume_exception_type("TypeError"));
    }

    #[test]
    fn invalid_globals_fail_without_mutating_locals() {
        let mut context = RimeraContext::default();
        context.initialize_kernel().unwrap();
        let locals = operations::dictionary(&mut context, &[], &[]).unwrap();
        context.add_context_root(locals);
        let source = operations::string(&mut context, "answer = 42").unwrap();
        context.add_context_root(source);
        assert!(
            execute(
                &mut context,
                &[source, RValue::small_int(1), locals],
                &[],
                false
            )
            .is_err()
        );
        assert!(context.namespace_value(locals, "answer").is_none());
        assert!(context.namespace_value(locals, "__builtins__").is_none());
        assert!(context.consume_exception_type("TypeError"));
    }
}
