use std::fmt;

use num_traits::ToPrimitive;
use rimera_abi::{RDynamicCompileMode, RNativeFunction, RStatus, RValue};

use crate::context::RimeraContext;
use crate::object::{CodeObject, FunctionKind, FunctionObject};
use crate::heap::HeapObject;
use crate::operations;

pub struct NativeDynamicCode {
    pub address: usize,
    pub mode: RDynamicCompileMode,
    pub filename: String,
    pub flags: u32,
    pub owner: Box<dyn fmt::Debug>,
}

impl fmt::Debug for NativeDynamicCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeDynamicCode").field("filename", &self.filename)
            .field("mode", &self.mode).finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct DynamicCompileError {
    pub exception_type: &'static str,
    pub message: String,
    pub offset: usize,
}

pub type DynamicCompiler = fn(&str, &str, RDynamicCompileMode, u32, i32)
    -> Result<NativeDynamicCode, DynamicCompileError>;

#[derive(Debug, Default)]
pub(crate) struct DynamicState {
    pub compiler: Option<DynamicCompiler>,
    pub native_units: Vec<NativeDynamicCode>,
}

impl RimeraContext {
    pub fn install_dynamic_compiler(&mut self, compiler: DynamicCompiler) {
        self.enable_dynamic_compilation();
        self.dynamic.compiler = Some(compiler);
    }

    pub(crate) fn compile_dynamic(&mut self, source: &str, filename: &str,
        mode: RDynamicCompileMode, flags: u32, optimize: i32) -> Result<RValue, String> {
        let Some(compiler) = self.dynamic.compiler else {
            return self.raise_error("RuntimeError", "dynamic compiler service is not linked for this artifact");
        };
        let native = match compiler(source, filename, mode, flags, optimize) {
            Ok(native) => native,
            Err(error) => {
                self.raise_builtin(error.exception_type, error.message.clone())?;
                if error.exception_type == "SyntaxError" {
                    let exception = self.raised.ok_or("syntax exception unavailable")?;
                    self.with_temporary_roots(&[exception], |context| {
                        let offset = error.offset.min(source.len());
                        let line = source[..offset].bytes().filter(|byte| *byte == b'\n').count() + 1;
                        let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
                        let line_end = source[offset..].find('\n').map_or(source.len(), |index| offset + index + 1);
                        for (name, text) in [("filename", filename), ("text", &source[line_start..line_end]), ("msg", &error.message)] {
                            let value = operations::string(context, text)?;
                            context.attribute_set(exception, name, value)?;
                        }
                        context.attribute_set(exception, "lineno", RValue::small_int(line as i64))?;
                        context.attribute_set(exception, "offset", RValue::small_int(source[line_start..offset].chars().count() as i64 + 1))?;
                        context.attribute_set(exception, "end_lineno", RValue::NONE)?;
                        context.attribute_set(exception, "end_offset", RValue::NONE)
                    })?;
                }
                return Err(error.message);
            }
        };
        let code = CodeObject {
            dynamic_mode: Some(mode),
            flags_override: Some(native.flags),
            code_address: native.address,
            kind: FunctionKind::Normal,
            name: "<module>".to_owned(), qualified_name: "<module>".to_owned(),
            parameters: Box::new([]), filename: filename.to_owned(), first_line: 1,
            local_names: Box::new([]), cell_names: Box::new([]), free_names: Box::new([]),
        };
        let value = self.allocate(HeapObject::Code(code))?;
        self.dynamic.native_units.push(native);
        Ok(value)
    }

    pub(crate) fn execute_dynamic(&mut self, code_value: RValue, globals: RValue,
        locals: RValue, closure: Option<RValue>, eval: bool) -> Result<RValue, String> {
        let Some(HeapObject::Code(code)) = self.heap.get(code_value) else {
            return self.raise_error("TypeError", "expected a code object");
        };
        let code = code.clone();
        if eval && !code.free_names.is_empty() {
            return self.raise_error("TypeError", "code object passed to eval() may not contain free variables");
        }
        let closure = closure.filter(|value| *value != RValue::NONE);
        if !code.free_names.is_empty() || closure.is_some() {
            let valid = closure.is_some_and(|value| matches!(self.heap.get(value),
                Some(HeapObject::Tuple(cells)) if cells.len() == code.free_names.len()
                && cells.iter().all(|cell| matches!(self.heap.get(*cell), Some(HeapObject::Cell(_))))));
            if code.free_names.is_empty() || !valid {
                return self.raise_error("TypeError", &format!("code object requires a closure of exactly length {}", code.free_names.len()));
            }
        }
        if !code.parameters.is_empty() {
            return self.raise_error("TypeError", "code object requires positional arguments");
        }
        let mut roots = vec![code_value, globals, locals];
        roots.extend(closure);
        self.with_temporary_roots(&roots, |context| {
            if context.namespace_value(globals, "__builtins__").is_none() {
                let builtins = context.builtins().ok_or("builtins are unavailable")?;
                context.namespace_set(globals, "__builtins__", builtins)?;
            }
            let function = context.allocate(HeapObject::Function(FunctionObject {
                code: code_value, fast_call: code.fast_call_metadata(), globals,
                name: code.name.clone(), qualified_name: code.qualified_name.clone(), closure,
                defaults: None, keyword_defaults: None, annotations: None, type_params: None,
            }))?;
            context.with_temporary_roots(&[function], |context| {
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
                let arguments = if code.dynamic_mode.is_some() { &[locals][..] } else { &[][..] };
                let status = unsafe { native(std::ptr::from_mut(context).cast(), &function,
                    arguments.as_ptr(), arguments.len(), &mut output) };
                context.pop_active_call();
                context.pop_module_namespace(globals);
                if status == RStatus::Ok { Ok(if eval { output } else { RValue::NONE }) }
                else { Err("dynamic code raised an exception".to_owned()) }
            })
        })
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

pub(crate) fn compile(context: &mut RimeraContext, positional: &[RValue],
    keywords: &[(String, RValue)]) -> Result<RValue, String> {
    let names = ["source", "filename", "mode", "flags", "dont_inherit", "optimize"];
    if positional.len() > names.len() { return Err("compile() takes at most 6 arguments".to_owned()); }
    let mut args = [None; 6];
    for (slot, value) in args.iter_mut().zip(positional) { *slot = Some(*value); }
    for (name, value) in keywords {
        let Some(index) = names.iter().position(|candidate| candidate == name) else {
            return Err(format!("'{name}' is an invalid keyword argument for compile()"));
        };
        if args[index].replace(*value).is_some() { return Err(format!("argument for compile() given by name ('{name}') and position")); }
    }
    if args[..3].iter().any(Option::is_none) { return Err("compile() missing required argument".to_owned()); }
    let source = source_string(context, args[0].unwrap())?;
    let filename = source_string(context, args[1].unwrap())?;
    let mode = operations::string_value(context, args[2].unwrap()).ok_or("compile() mode must be a string")?;
    let mode = match mode {
        "exec" => RDynamicCompileMode::Exec, "eval" => RDynamicCompileMode::Eval,
        "single" => RDynamicCompileMode::Single,
        _ => return context.raise_error("ValueError", "compile() mode must be 'exec', 'eval' or 'single'"),
    };
    let flags = match args[3] {
        Some(value) => operations::index_integer(context, value)?.to_u32().ok_or("compile(): unrecognised flags")?,
        None => 0,
    };
    if let Some(value) = args[4] { operations::index_integer(context, value)?; }
    let optimize = match args[5] {
        Some(value) => operations::index_integer(context, value)?.to_i32().ok_or("compile(): invalid optimize value")?,
        None => -1,
    };
    context.compile_dynamic(&source, &filename, mode, flags, optimize)
}

pub(crate) fn execute(context: &mut RimeraContext, positional: &[RValue],
    keywords: &[(String, RValue)], eval: bool) -> Result<RValue, String> {
    if positional.is_empty() || positional.len() > 3 { return Err("expected from 1 to 3 positional arguments".to_owned()); }
    if keywords.len() > 1 || keywords.iter().any(|(name, _)| eval || name != "closure") { return Err("invalid keyword argument".to_owned()); }
    let globals = positional.get(1).copied().filter(|v| *v != RValue::NONE);
    let locals = positional.get(2).copied().filter(|v| *v != RValue::NONE);
    let locals = match locals.or(globals) { Some(value) => value, None => context.current_locals()? };
    let globals = globals.or(context.globals()).ok_or("globals are unavailable")?;
    if !matches!(context.heap.get(globals), Some(HeapObject::Dictionary(_) | HeapObject::ValueDictionary(_))) {
        return context.raise_error("TypeError", "globals must be a dict");
    }
    if !matches!(context.heap.get(locals), Some(HeapObject::Dictionary(_) | HeapObject::ValueDictionary(_)))
        && !context.has_special_method_slot(locals, "__getitem__")? {
        return context.raise_error("TypeError", "locals must be a mapping");
    }
    let closure = keywords.first().map(|(_, value)| *value);
    let mut roots = vec![globals, locals];
    roots.extend_from_slice(positional);
    roots.extend(closure);
    context.with_temporary_roots(&roots, |context| {
        let code = if matches!(context.heap.get(positional[0]), Some(HeapObject::Code(_))) {
            positional[0]
        } else {
            if closure.is_some_and(|value| value != RValue::NONE) { return Err("closure can only be used when source is a code object".to_owned()); }
            let source = source_string(context, positional[0])?;
            let source = if eval { source.trim_start_matches([' ', '\t']) } else { &source };
            context.compile_dynamic(source, "<string>", if eval { RDynamicCompileMode::Eval } else { RDynamicCompileMode::Exec }, 0, -1)?
        };
        context.execute_dynamic(code, globals, locals, closure, eval)
    })
}
