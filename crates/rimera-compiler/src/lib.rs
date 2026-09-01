pub mod codegen;
pub mod core;
pub mod hir;
pub mod link;
pub mod lir;
pub mod lower;
pub mod mir;
pub mod project;
pub mod resolve;
pub mod sema;
pub mod syntax;

use std::fs;
use std::path::{Path, PathBuf};

use crate::core::{Diagnostic, DiagnosticSet, Span};
use crate::project::{BuildArtifact, BuildRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildStage {
    AnalyzeSource,
    AnalyzeSemantics,
    LowerMir,
    EmitObject,
    LinkRuntime,
}

impl BuildStage {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::AnalyzeSource => "Analyzing source [☺︎]",
            Self::AnalyzeSemantics => "Resolving semantics [♛]",
            Self::LowerMir => "Lowering native IR [✦]",
            Self::EmitObject => "Emitting object file (.o) [♜]",
            Self::LinkRuntime => "Linking runtime [♣]",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildProgress {
    Started(BuildStage),
    Finished(BuildStage),
    Trace(String),
}

pub fn build(request: BuildRequest) -> Result<BuildArtifact, DiagnosticSet> {
    build_with_progress(request, |_| {})
}

pub fn build_with_progress(
    request: BuildRequest,
    mut report: impl FnMut(BuildProgress),
) -> Result<BuildArtifact, DiagnosticSet> {
    if is_debug_ir_dump(&request.entry) {
        return Err(error(
            "RIM-INPUT-002",
            "Rimera IR dumps are read-only debugging artifacts; compile the original Python entry instead",
            &request.entry,
        ));
    }
    if request.target.as_str() != core::TargetTriple::MACOS_ARM64 {
        return Err(error(
            "RIM-TARGET-001",
            "the native backend currently targets macOS arm64",
            &request.entry,
        ));
    }
    trace(
        &mut report,
        request.debug,
        format!("entry: {}", request.entry.display()),
    );
    trace(
        &mut report,
        request.debug,
        format!(
            "target: {} · profile: {:?}",
            request.target.as_str(),
            request.profile
        ),
    );
    report(BuildProgress::Started(BuildStage::AnalyzeSource));
    let source = fs::read_to_string(&request.entry).map_err(|cause| {
        error(
            "RIM-INPUT-001",
            format!("failed to read source: {cause}"),
            &request.entry,
        )
    })?;
    let syntax = syntax::parse(&request.entry, &source)?;
    report(BuildProgress::Finished(BuildStage::AnalyzeSource));
    trace(
        &mut report,
        request.debug,
        format!("parsed {} top-level statement(s)", syntax.statements.len()),
    );

    report(BuildProgress::Started(BuildStage::AnalyzeSemantics));
    let _graph = resolve::single_file(request.entry.clone());
    let hir = sema::analyze(&request.entry, &syntax)?;
    report(BuildProgress::Finished(BuildStage::AnalyzeSemantics));

    report(BuildProgress::Started(BuildStage::LowerMir));
    let mir = lower::lower(&hir).map_err(|cause| error("RIM-IR-001", cause, &request.entry))?;
    report(BuildProgress::Finished(BuildStage::LowerMir));
    trace(
        &mut report,
        request.debug,
        format!(
            "MIR: {} function(s), {} block(s), {} value(s)",
            mir.functions.len(),
            mir.functions
                .iter()
                .map(|function| function.blocks.len())
                .sum::<usize>(),
            mir.functions
                .iter()
                .map(|function| u64::from(function.value_count))
                .sum::<u64>()
        ),
    );
    build_mir_with_progress(&request, &mir, report)
}

pub fn build_mir(
    request: &BuildRequest,
    mir: &mir::Program,
) -> Result<BuildArtifact, DiagnosticSet> {
    build_mir_with_progress(request, mir, |_| {})
}

fn build_mir_with_progress(
    request: &BuildRequest,
    mir: &mir::Program,
    mut report: impl FnMut(BuildProgress),
) -> Result<BuildArtifact, DiagnosticSet> {
    mir::verify(mir).map_err(|cause| error("RIM-IR-001", cause, &request.entry))?;
    let lir = lir::select(mir);
    let cache_dir = cache_dir(request)?;
    let ir_dump = request.debug.then(|| debug_ir_path(request, &cache_dir));
    if let Some(ir_dump) = &ir_dump {
        fs::create_dir_all(ir_dump.parent().expect("IR cache path has a parent"))
            .map_err(|cause| error("RIM-OUTPUT-001", cause.to_string(), &request.entry))?;
        fs::write(ir_dump, mir::render_python(mir))
            .map_err(|cause| error("RIM-OUTPUT-001", cause.to_string(), &request.entry))?;
        trace(&mut report, true, format!("IR dump: {}", ir_dump.display()));
    }
    report(BuildProgress::Started(BuildStage::EmitObject));
    let object_bytes = codegen::emit_object(
        &lir,
        request.profile == core::BuildProfile::Release,
        request.heap_limit_bytes,
    )
    .map_err(|cause| error("RIM-CODEGEN-001", cause, &request.entry))?;
    let object = object_path(request, &cache_dir);
    fs::create_dir_all(object.parent().expect("object cache path has a parent"))
        .map_err(|cause| error("RIM-OUTPUT-001", cause.to_string(), &request.entry))?;
    fs::write(&object, object_bytes)
        .map_err(|cause| error("RIM-OUTPUT-001", cause.to_string(), &request.entry))?;
    report(BuildProgress::Finished(BuildStage::EmitObject));
    trace(
        &mut report,
        request.debug,
        format!("object: {}", object.display()),
    );

    let runtime = runtime_archive(request.profile).ok_or_else(|| {
        error(
            "RIM-LINK-001",
            "Rust runtime archive is missing; build the Rimera workspace first",
            &request.entry,
        )
    })?;
    trace(
        &mut report,
        request.debug,
        format!("runtime archive: {}", runtime.display()),
    );
    report(BuildProgress::Started(BuildStage::LinkRuntime));
    if let Err(cause) = link::link(&object, &runtime, &request.output, request.profile) {
        return Err(error("RIM-LINK-001", cause, &request.entry));
    }
    report(BuildProgress::Finished(BuildStage::LinkRuntime));
    Ok(BuildArtifact {
        executable: request.output.clone(),
        object,
        cache_dir,
        ir_dump,
    })
}

fn runtime_archive(profile: core::BuildProfile) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("RIMERA_RUNTIME_ARCHIVE").map(PathBuf::from) {
        return path.is_file().then_some(path);
    }
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let profile = if profile == core::BuildProfile::Release {
        "release"
    } else {
        "debug"
    };
    let path = workspace
        .join("target")
        .join(profile)
        .join("librimera_runtime.a");
    path.is_file().then_some(path)
}

fn cache_dir(request: &BuildRequest) -> Result<PathBuf, DiagnosticSet> {
    let cache_dir = std::env::var_os("RIMERA_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| request.project_root.join(".rimera"));
    if cache_dir.file_name().is_some_and(|name| name == ".rimera") {
        Ok(cache_dir)
    } else {
        Err(error(
            "RIM-CONFIG-001",
            "RIMERA_CACHE_DIR must name a directory called `.rimera`",
            &request.entry,
        ))
    }
}

fn is_debug_ir_dump(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".ir.py"))
        && path
            .ancestors()
            .any(|ancestor| ancestor.file_name().is_some_and(|name| name == ".rimera"))
}

fn object_path(request: &BuildRequest, cache_dir: &Path) -> PathBuf {
    cache_dir
        .join("objects")
        .join(format!("{}.o", cache_key(request)))
}

fn debug_ir_path(request: &BuildRequest, cache_dir: &Path) -> PathBuf {
    cache_dir
        .join("ir")
        .join(format!("{}.ir.py", cache_key(request)))
}

fn cache_key(request: &BuildRequest) -> String {
    let key = format!(
        "{}\0{}\0{}\0{:?}\0{}",
        request.entry.display(),
        request.output.display(),
        request.target.as_str(),
        request.profile,
        request.heap_limit_bytes.unwrap_or(0)
    );
    format!("{:016x}", fnv1a_64(&key))
}

fn fnv1a_64(value: &str) -> u64 {
    value.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn trace(report: &mut impl FnMut(BuildProgress), enabled: bool, message: String) {
    if enabled {
        report(BuildProgress::Trace(message));
    }
}

fn error(code: &'static str, message: impl Into<String>, path: &Path) -> DiagnosticSet {
    DiagnosticSet::one(Diagnostic::new(code, message, path, Span::default()))
}
