use std::path::{Path, PathBuf};

use rimera_abi::RDynamicCompileMode;

use crate::core::{Diagnostic, DiagnosticSet, Span};
use crate::{hir, lower, mir, resolve, sema, syntax};

/// Gate 11 source forms accepted by the compiler service before runtime
/// adapters turn Python str/bytes-like objects into borrowed source bytes.
#[derive(Debug, Clone, Copy)]
pub enum DynamicSource<'a> {
    Text(&'a str),
    Utf8Bytes(&'a [u8]),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicCompileOptions {
    pub filename: String,
    pub mode: RDynamicCompileMode,
    pub flags: u32,
    pub dont_inherit: bool,
    pub optimize: i8,
}

impl DynamicCompileOptions {
    #[must_use]
    pub fn cpython_default(filename: impl Into<String>, mode: RDynamicCompileMode) -> Self {
        Self {
            filename: filename.into(),
            mode,
            flags: 0,
            dont_inherit: false,
            optimize: -1,
        }
    }
}

/// Immutable, Python-visible portion of one dynamically compiled code object.
/// Native mapping/cache ownership is deliberately separate and belongs to
/// Gate 11 Slice 5.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicCodeMetadata {
    pub mode: RDynamicCompileMode,
    pub filename: String,
    pub first_line: u32,
    pub flags: u32,
    pub optimize: i8,
    pub source_hash: u64,
    pub name: String,
    pub qualified_name: String,
}

/// One authoritative dynamic compilation unit. It has already passed Rimera's
/// normal parser, semantic analyzer, HIR lowering, MIR lowering, and MIR
/// verifier before it may be published as a managed code object.
#[derive(Debug)]
pub struct DynamicCompilation {
    pub metadata: DynamicCodeMetadata,
    pub hir: hir::Module,
    pub mir: mir::Program,
}

fn dynamic_error(
    code: &'static str,
    message: impl Into<String>,
    path: &Path,
) -> DiagnosticSet {
    DiagnosticSet::one(Diagnostic::new(code, message, path, Span::default()))
}

fn validate_options(options: &DynamicCompileOptions, path: &Path) -> Result<(), DiagnosticSet> {
    // Slice 2 intentionally starts with the semantically inert flag set. Flags
    // such as ONLY_AST, top-level await, type comments, and future features are
    // not silently ignored: accepting them requires their owning slice.
    if options.flags != 0 {
        return Err(dynamic_error(
            "RIM-DYN-FLAG-001",
            "compile(): unrecognised flags",
            path,
        ));
    }
    if !matches!(options.optimize, -1..=2) {
        return Err(dynamic_error(
            "RIM-DYN-OPT-001",
            "compile(): invalid optimize value",
            path,
        ));
    }
    Ok(())
}

fn source_text<'a>(source: DynamicSource<'a>, path: &Path) -> Result<&'a str, DiagnosticSet> {
    match source {
        DynamicSource::Text(source) => Ok(source),
        DynamicSource::Utf8Bytes(source) => std::str::from_utf8(source).map_err(|_| {
            dynamic_error(
                "RIM-DYN-SOURCE-001",
                "compile() source bytes are not valid UTF-8",
                path,
            )
        }),
    }
}

/// Compiles dynamic Python source through the same semantic and MIR pipeline as
/// a normal Rimera build. This API never emits/loads native memory itself and
/// never shells out to CPython or interprets Python AST/bytecode.
pub fn compile_source(
    source: DynamicSource<'_>,
    options: &DynamicCompileOptions,
) -> Result<DynamicCompilation, DiagnosticSet> {
    let path = PathBuf::from(&options.filename);
    validate_options(options, &path)?;
    let source = source_text(source, &path)?;
    if source.contains('\0') {
        return Err(dynamic_error("RIM-DYN-SOURCE-002", "source code string cannot contain null bytes", &path));
    }
    let mut syntax = syntax::parse_mode(&path, source, options.mode)?;
    optimize_suite(&mut syntax.statements, options.optimize, true);
    let mut hir = sema::analyze_with_dynamic_compilation(&path, &syntax, true)?;
    if options.mode == RDynamicCompileMode::Single { mark_display(&mut hir.statements); }
    if options.mode == RDynamicCompileMode::Eval {
        if let Some(hir::Statement { kind, .. }) = hir.statements.last_mut() {
            if let hir::StatementKind::Expression(value) = kind {
                *kind = hir::StatementKind::Return { value: Some(value.clone()) };
            }
        }
    }
    let mut mir = lower::lower_dynamic(&hir)
        .map_err(|error| dynamic_error("RIM-DYN-MIR-001", error, &path))?;
    let mut globals = std::collections::BTreeSet::new();
    collect_globals(&syntax.statements, &mut globals);
    let entry = &mut mir.functions[mir.entry.0 as usize];
    entry.kind = mir::FunctionKind::Python;
    let namespace = mir::ValueId(entry.value_count);
    entry.value_count += 1;
    entry.parameters.push(mir::Parameter {
        value: namespace, name: "<namespace>".to_owned(),
        kind: rimera_abi::RParameterKind::PositionalOnly, has_default: false,
    });
    for block in &mut entry.blocks {
        for operation in &mut block.operations {
            operation.kind = match &operation.kind {
                mir::OperationKind::AnnotationsEnsure { namespace: None } =>
                    mir::OperationKind::AnnotationsEnsure { namespace: Some(namespace) },
                mir::OperationKind::GlobalGet { dest, name } if !globals.contains(name) =>
                    mir::OperationKind::ClassNameGet { dest: *dest, namespace, name: name.clone() },
                mir::OperationKind::GlobalSet { name, value } if !globals.contains(name) =>
                    mir::OperationKind::ClassNamespaceSet { namespace, name: name.clone(), value: *value },
                mir::OperationKind::GlobalDelete { name } if !globals.contains(name) =>
                    mir::OperationKind::ClassNamespaceDelete { namespace, name: name.clone() },
                other => other.clone(),
            };
        }
        if matches!(block.terminator, mir::Terminator::Return { code: 0 }) {
            block.terminator = mir::Terminator::ReturnValue { value: None };
        }
    }
    for function in &mut mir.functions {
        for block in &mut function.blocks {
            for operation in &mut block.operations {
                if let mir::OperationKind::GlobalGet { dest, name } | mir::OperationKind::ClassNameGet { dest, name, .. } = &operation.kind {
                    if name == "__debug__" {
                        operation.kind = mir::OperationKind::Constant { dest: *dest, value: mir::Constant::Bool(options.optimize <= 0) };
                    }
                }
            }
        }
    }
    mir::verify(&mir)
        .map_err(|error| dynamic_error("RIM-DYN-MIR-002", error, &path))?;
    let metadata = DynamicCodeMetadata {
        mode: options.mode,
        filename: options.filename.clone(),
        first_line: 1,
        flags: options.flags,
        optimize: options.optimize,
        source_hash: resolve::source_hash(source.as_bytes()),
        name: "<module>".to_owned(),
        qualified_name: "<module>".to_owned(),
    };
    Ok(DynamicCompilation { metadata, hir, mir })
}

fn mark_display(statements: &mut [hir::Statement]) {
    use hir::StatementKind as S;
    for statement in statements {
        match &mut statement.kind {
            S::Expression(value) => statement.kind = S::Display(value.clone()),
            S::If { then_body, else_body, .. } => { mark_display(then_body); mark_display(else_body); }
            S::While { body, .. } | S::With { body, .. } => mark_display(body),
            S::For { body, else_body, .. } => { mark_display(body); mark_display(else_body); }
            S::Try { body, handlers, else_body, finally_body, .. } => {
                mark_display(body); mark_display(else_body); mark_display(finally_body);
                for handler in handlers { mark_display(&mut handler.body); }
            }
            S::Match { cases, .. } => for case in cases { mark_display(&mut case.body); },
            _ => {}
        }
    }
}

fn optimize_suite(statements: &mut Vec<syntax::Statement>, optimize: i8, docstring: bool) {
    use syntax::StatementKind as S;
    if optimize == 2 && docstring && statements.first().is_some_and(|statement|
        matches!(&statement.kind, S::Expression(syntax::Expression { kind: syntax::ExpressionKind::String(_), .. }))) {
        statements.remove(0);
    }
    if optimize > 0 { statements.retain(|statement| !matches!(statement.kind, S::Assert { .. })); }
    for statement in statements {
        match &mut statement.kind {
            S::FunctionDef { body, .. } | S::ClassDef { body, .. } => optimize_suite(body, optimize, true),
            S::If { then_body, else_body, .. } => { optimize_suite(then_body, optimize, false); optimize_suite(else_body, optimize, false); }
            S::While { body, .. } | S::With { body, .. } => optimize_suite(body, optimize, false),
            S::For { body, else_body, .. } => { optimize_suite(body, optimize, false); optimize_suite(else_body, optimize, false); }
            S::Try { body, handlers, else_body, finally_body, .. } => {
                optimize_suite(body, optimize, false); optimize_suite(else_body, optimize, false); optimize_suite(finally_body, optimize, false);
                for handler in handlers { optimize_suite(&mut handler.body, optimize, false); }
            }
            S::Match { cases, .. } => for case in cases { optimize_suite(&mut case.body, optimize, false); },
            _ => {}
        }
    }
}

fn collect_globals(statements: &[syntax::Statement], names: &mut std::collections::BTreeSet<String>) {
    use syntax::StatementKind as S;
    for statement in statements {
        match &statement.kind {
            S::Global(items) => names.extend(items.iter().cloned()),
            S::If { then_body, else_body, .. } => {
                collect_globals(then_body, names); collect_globals(else_body, names);
            }
            S::For { body, else_body, .. } => {
                collect_globals(body, names); collect_globals(else_body, names);
            }
            S::While { body, .. } | S::With { body, .. } => collect_globals(body, names),
            S::Try { body, handlers, else_body, finally_body, .. } => {
                collect_globals(body, names); collect_globals(else_body, names);
                collect_globals(finally_body, names);
                for handler in handlers { collect_globals(&handler.body, names); }
            }
            S::Match { cases, .. } => for case in cases { collect_globals(&case.body, names); },
            _ => {}
        }
    }
}

struct NativeOwner(Option<cranelift_jit::JITModule>);

impl std::fmt::Debug for NativeOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str("NativeOwner") }
}

impl Drop for NativeOwner {
    fn drop(&mut self) {
        if let Some(module) = self.0.take() {
            // SAFETY: the runtime drops its native units only after all execution ends.
            unsafe { module.free_memory() };
        }
    }
}

fn compile_native(source: &str, filename: &str, mode: RDynamicCompileMode, flags: u32,
    optimize: i32) -> Result<rimera_runtime::NativeDynamicCode, rimera_runtime::DynamicCompileError> {
    use rimera_runtime::DynamicCompileError;
    let options = DynamicCompileOptions { filename: filename.to_owned(), mode, flags,
        dont_inherit: true, optimize: i8::try_from(optimize).unwrap_or(127) };
    let unit = compile_source(DynamicSource::Text(source), &options).map_err(|errors| {
        let error = &errors.as_slice()[0];
        DynamicCompileError {
            exception_type: if error.code.starts_with("RIM-DYN-FLAG") || error.code.starts_with("RIM-DYN-OPT") { "ValueError" } else { "SyntaxError" },
            message: error.message.clone(), offset: error.span.start as usize,
        }
    })?;
    let (module, address) = crate::codegen::emit_jit(&crate::lir::select(&unit.mir),
        &crate::runtime_symbols::symbols()).map_err(|message| DynamicCompileError {
            exception_type: "RuntimeError", message, offset: 0,
        })?;
    Ok(rimera_runtime::NativeDynamicCode { address: address as usize, mode,
        filename: filename.to_owned(), flags: 0, owner: Box::new(NativeOwner(Some(module))) })
}

/// Installs the native compiler only in an artifact linked with this service.
///
/// # Safety
/// `context` must point to a live, exclusively borrowed runtime context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rimera_dynamic_compiler_install(context: *mut rimera_runtime::RimeraContext) -> rimera_abi::RStatus {
    if let Some(context) = unsafe { context.as_mut() } {
        context.install_dynamic_compiler(compile_native);
        rimera_abi::RStatus::Ok
    } else {
        rimera_abi::RStatus::InvalidArgument
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile(source: &str, mode: RDynamicCompileMode) -> DynamicCompilation {
        compile_source(
            DynamicSource::Text(source),
            &DynamicCompileOptions::cpython_default("<gate11>", mode),
        )
        .unwrap()
    }

    #[test]
    fn all_three_compile_modes_use_verified_mir() {
        for (source, mode) in [
            ("value = 1", RDynamicCompileMode::Exec),
            ("1 + 2", RDynamicCompileMode::Eval),
            ("1 + 2", RDynamicCompileMode::Single),
        ] {
            let unit = compile(source, mode);
            assert_eq!(unit.metadata.mode, mode);
            assert_eq!(unit.metadata.filename, "<gate11>");
            assert_eq!(unit.metadata.first_line, 1);
            assert_eq!(unit.metadata.name, "<module>");
            assert_eq!(unit.metadata.qualified_name, "<module>");
            mir::verify(&unit.mir).unwrap();
        }
    }

    #[test]
    fn mode_grammar_matches_cpython_shape() {
        assert!(compile_source(
            DynamicSource::Text("value = 1"),
            &DynamicCompileOptions::cpython_default("<eval>", RDynamicCompileMode::Eval),
        )
        .is_err());
        assert!(compile_source(
            DynamicSource::Text("first = 1\nsecond = 2"),
            &DynamicCompileOptions::cpython_default("<single>", RDynamicCompileMode::Single),
        )
        .is_err());
        compile("1 + 2", RDynamicCompileMode::Exec);
    }

    #[test]
    fn dynamic_source_can_reference_nested_dynamic_builtins() {
        for builtin in ["compile", "eval", "exec"] {
            let source = format!("saved = {builtin}");
            compile(&source, RDynamicCompileMode::Exec);
        }
    }

    #[test]
    fn metadata_is_deterministic_and_source_sensitive() {
        let first = compile("answer = 42", RDynamicCompileMode::Exec);
        let again = compile("answer = 42", RDynamicCompileMode::Exec);
        let changed = compile("answer = 43", RDynamicCompileMode::Exec);
        assert_eq!(first.metadata, again.metadata);
        assert_ne!(first.metadata.source_hash, changed.metadata.source_hash);
    }

    #[test]
    fn utf8_bytes_share_the_text_pipeline() {
        let options = DynamicCompileOptions::cpython_default("<bytes>", RDynamicCompileMode::Eval);
        let text = compile_source(DynamicSource::Text("40 + 2"), &options).unwrap();
        let bytes = compile_source(DynamicSource::Utf8Bytes(b"40 + 2"), &options).unwrap();
        assert_eq!(text.metadata, bytes.metadata);
    }

    #[test]
    fn unsupported_flags_and_optimization_fail_before_publication() {
        let mut options =
            DynamicCompileOptions::cpython_default("<flags>", RDynamicCompileMode::Exec);
        options.flags = 1;
        let diagnostics = compile_source(DynamicSource::Text("pass"), &options).unwrap_err();
        assert_eq!(diagnostics.as_slice()[0].code, "RIM-DYN-FLAG-001");
        assert_eq!(diagnostics.as_slice()[0].message, "compile(): unrecognised flags");

        options.flags = 0;
        options.optimize = 3;
        let diagnostics = compile_source(DynamicSource::Text("pass"), &options).unwrap_err();
        assert_eq!(diagnostics.as_slice()[0].code, "RIM-DYN-OPT-001");
        assert_eq!(diagnostics.as_slice()[0].message, "compile(): invalid optimize value");
    }

    #[test]
    fn invalid_source_bytes_fail_without_a_compiled_unit() {
        let options = DynamicCompileOptions::cpython_default("<bytes>", RDynamicCompileMode::Exec);
        let diagnostics =
            compile_source(DynamicSource::Utf8Bytes(&[0xff, 0xfe]), &options).unwrap_err();
        assert_eq!(diagnostics.as_slice()[0].code, "RIM-DYN-SOURCE-001");
    }
}
