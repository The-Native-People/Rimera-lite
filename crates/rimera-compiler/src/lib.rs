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

use std::fs::{self, File};
use std::io::Read;
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
            Self::LinkRuntime => "Linking runtime with clang [♣]",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildProgress {
    Started(BuildStage),
    Finished(BuildStage),
    Measured(ProgressMeasure),
    Trace(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressMeasure {
    pub stage: BuildStage,
    pub completed: u64,
    pub total: Option<u64>,
    pub unit: &'static str,
    pub detail: String,
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
    let source = read_source_with_progress(&request.entry, &mut report)?;
    let syntax = syntax::parse(&request.entry, &source)?;
    let source_lines = source.lines().count();
    report(BuildProgress::Measured(ProgressMeasure {
        stage: BuildStage::AnalyzeSource,
        completed: source.len() as u64,
        total: Some(source.len() as u64),
        unit: "source bytes",
        detail: format!(
            "{} statements · {} lines",
            format_number(syntax.statements.len() as u64),
            format_number(source_lines as u64)
        ),
    }));
    report(BuildProgress::Finished(BuildStage::AnalyzeSource));
    trace(
        &mut report,
        request.debug,
        format!("parsed {} top-level statement(s)", syntax.statements.len()),
    );

    report(BuildProgress::Started(BuildStage::AnalyzeSemantics));
    let _graph = resolve::single_file(request.entry.clone());
    let hir = sema::analyze(&request.entry, &syntax)?;
    report(BuildProgress::Measured(ProgressMeasure {
        stage: BuildStage::AnalyzeSemantics,
        completed: hir.statements.len() as u64,
        total: Some(hir.statements.len() as u64),
        unit: "HIR statements",
        detail: "analyzed".to_owned(),
    }));
    report(BuildProgress::Finished(BuildStage::AnalyzeSemantics));

    report(BuildProgress::Started(BuildStage::LowerMir));
    let mir = lower::lower(&hir).map_err(|cause| error("RIM-IR-001", cause, &request.entry))?;
    let mir_blocks = mir
        .functions
        .iter()
        .map(|function| function.blocks.len())
        .sum::<usize>();
    let mir_values = mir
        .functions
        .iter()
        .map(|function| u64::from(function.value_count))
        .sum::<u64>();
    report(BuildProgress::Measured(ProgressMeasure {
        stage: BuildStage::LowerMir,
        completed: mir.functions.len() as u64,
        total: Some(mir.functions.len() as u64),
        unit: "native functions",
        detail: format!(
            "{} blocks · {} values",
            format_number(mir_blocks as u64),
            format_number(mir_values)
        ),
    }));
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
    fs::write(&object, &object_bytes)
        .map_err(|cause| error("RIM-OUTPUT-001", cause.to_string(), &request.entry))?;
    report(BuildProgress::Measured(ProgressMeasure {
        stage: BuildStage::EmitObject,
        completed: 1,
        total: Some(1),
        unit: "object",
        detail: format!(
            "{} · {} native functions",
            format_bytes(object_bytes.len() as u64),
            format_number(mir.functions.len() as u64)
        ),
    }));
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
    let runtime_size = fs::metadata(&runtime)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    report(BuildProgress::Measured(ProgressMeasure {
        stage: BuildStage::LinkRuntime,
        completed: 0,
        total: Some(1),
        unit: "executable",
        detail: format!("clang · runtime archive {}", format_bytes(runtime_size)),
    }));
    if let Err(cause) = link::link(&object, &runtime, &request.output, request.profile) {
        return Err(error("RIM-LINK-001", cause, &request.entry));
    }
    report(BuildProgress::Measured(ProgressMeasure {
        stage: BuildStage::LinkRuntime,
        completed: 1,
        total: Some(1),
        unit: "executable",
        detail: format!(
            "clang linked · {}",
            format_bytes(
                fs::metadata(&request.output)
                    .map(|metadata| metadata.len())
                    .unwrap_or(0)
            )
        ),
    }));
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

fn read_source_with_progress(
    path: &Path,
    report: &mut impl FnMut(BuildProgress),
) -> Result<String, DiagnosticSet> {
    let mut file = File::open(path).map_err(|cause| {
        error(
            "RIM-INPUT-001",
            format!("failed to read source: {cause}"),
            path,
        )
    })?;
    let total = file.metadata().ok().map(|metadata| metadata.len());
    let mut source = Vec::with_capacity(
        total
            .and_then(|bytes| usize::try_from(bytes).ok())
            .unwrap_or(0),
    );
    let mut buffer = [0_u8; 64 * 1024];
    let mut completed = 0_u64;
    loop {
        let read = file.read(&mut buffer).map_err(|cause| {
            error(
                "RIM-INPUT-001",
                format!("failed to read source: {cause}"),
                path,
            )
        })?;
        if read == 0 {
            break;
        }
        source.extend_from_slice(&buffer[..read]);
        completed += read as u64;
        report(BuildProgress::Measured(ProgressMeasure {
            stage: BuildStage::AnalyzeSource,
            completed,
            total,
            unit: "source bytes",
            detail: "read".to_owned(),
        }));
    }
    String::from_utf8(source).map_err(|cause| {
        error(
            "RIM-INPUT-001",
            format!("failed to read source: {cause}"),
            path,
        )
    })
}

fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    if bytes >= MIB {
        format!("{:.2} MiB ({bytes} B)", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.2} KiB ({bytes} B)", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn format_number(value: u64) -> String {
    let digits = value.to_string();
    let first_group = match digits.len() % 3 {
        0 => 3,
        length => length,
    };
    let mut formatted = String::with_capacity(digits.len() + (digits.len() - 1) / 3);
    formatted.push_str(&digits[..first_group]);
    for group in digits.as_bytes()[first_group..].chunks(3) {
        formatted.push(',');
        formatted.push_str(std::str::from_utf8(group).expect("number digits are valid UTF-8"));
    }
    formatted
}

fn error(code: &'static str, message: impl Into<String>, path: &Path) -> DiagnosticSet {
    DiagnosticSet::one(Diagnostic::new(code, message, path, Span::default()))
}
