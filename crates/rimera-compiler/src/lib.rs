pub mod codegen;
pub mod core;
pub mod dynamic;
pub mod hir;
pub mod link;
pub mod lir;
pub mod lower;
pub mod manifest;
pub mod mir;
pub mod project;
pub mod resolve;
mod runtime_symbols;
pub mod sema;
pub mod syntax;

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::core::{Diagnostic, DiagnosticSet, Span};
use crate::project::{AsyncBackend, BuildArtifact, BuildRequest};

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
    if matches!(
        request.async_backend,
        AsyncBackend::Monoio | AsyncBackend::Tokio
    ) {
        return Err(error(
            "RIM-ASYNC-001",
            format!(
                "async backend `{}` is unavailable for target `{}`; available backend: compio",
                request.async_backend.as_str(),
                request.target.as_str()
            ),
            &request.entry,
        ));
    }
    let locked_inputs = manifest::validate(
        &request.project_root,
        &request.target,
        &request.capabilities,
    )?;
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
    let parsed_entry = syntax::parse(&request.entry, &source)?;
    let entry_filename = parsed_entry.filename.clone();
    let resolved = resolve::discover_project_with_roots(
        &request.project_root,
        &request.entry,
        &request.module_search_roots,
    )?;
    manifest::validate_graph(&request.project_root, &resolved.graph, &locked_inputs)?;
    let reaches_async_runtime = resolved
        .graph
        .nodes()
        .any(|node| node.name.as_str() == "rimera.async_runtime");
    let linked_async_backend = reaches_async_runtime.then_some(AsyncBackend::Compio);
    trace(
        &mut report,
        request.debug,
        linked_async_backend.map_or_else(
            || "async backend: not linked".to_owned(),
            |backend| format!("async backend: {}", backend.as_str()),
        ),
    );
    let mut syntax = resolved
        .sources
        .get(&resolved.graph.entry_name)
        .map(|source| source.syntax.clone())
        .unwrap_or(parsed_entry);
    syntax.filename = entry_filename;
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
    trace(
        &mut report,
        request.debug,
        format!(
            "module graph: {} node(s), {} edge(s)",
            resolved.graph.nodes().count(),
            resolved.graph.edges().count()
        ),
    );
    let allow_dynamic_compilation = request.capabilities.dynamic_compilation();
    let hir =
        sema::analyze_with_dynamic_compilation(&request.entry, &syntax, allow_dynamic_compilation)?;
    let mut module_hir = BTreeMap::new();
    for (name, source) in &resolved.sources {
        if name.as_str() == "__main__" {
            continue;
        }
        let path = Path::new(&source.syntax.filename);
        module_hir.insert(
            name.as_str().to_owned(),
            sema::analyze_with_dynamic_compilation(
                path,
                &source.syntax,
                allow_dynamic_compilation,
            )?,
        );
    }
    let hir_statement_count = hir.statements.len()
        + module_hir
            .values()
            .map(|module| module.statements.len())
            .sum::<usize>();
    report(BuildProgress::Measured(ProgressMeasure {
        stage: BuildStage::AnalyzeSemantics,
        completed: hir_statement_count as u64,
        total: Some(hir_statement_count as u64),
        unit: "HIR statements",
        detail: "analyzed".to_owned(),
    }));
    report(BuildProgress::Finished(BuildStage::AnalyzeSemantics));

    report(BuildProgress::Started(BuildStage::LowerMir));
    let mir = lower::lower(&hir).map_err(|cause| error("RIM-IR-001", cause, &request.entry))?;
    let mut module_mir = BTreeMap::new();
    for (name, hir) in module_hir {
        let path = resolved
            .graph
            .nodes()
            .find(|node| node.name.as_str() == name)
            .and_then(|node| node.source.as_deref())
            .unwrap_or(&request.entry);
        let program = lower::lower(&hir).map_err(|cause| error("RIM-IR-001", cause, path))?;
        let source_hash = resolved
            .graph
            .nodes()
            .find(|node| node.name.as_str() == name)
            .and_then(|node| node.source_hash)
            .unwrap_or(0);
        module_mir.insert(
            name,
            NativeModule {
                program,
                source_hash,
            },
        );
    }
    let programs = std::iter::once(&mir).chain(module_mir.values().map(|module| &module.program));
    let mir_functions = programs
        .clone()
        .map(|program| program.functions.len())
        .sum::<usize>();
    let mir_blocks = programs
        .clone()
        .flat_map(|program| &program.functions)
        .map(|function| function.blocks.len())
        .sum::<usize>();
    let mir_values = programs
        .flat_map(|program| &program.functions)
        .map(|function| u64::from(function.value_count))
        .sum::<u64>();
    report(BuildProgress::Measured(ProgressMeasure {
        stage: BuildStage::LowerMir,
        completed: mir_functions as u64,
        total: Some(mir_functions as u64),
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
            mir_functions, mir_blocks, mir_values
        ),
    );
    let module_descriptors = resolved
        .graph
        .nodes()
        .filter(|node| {
            !matches!(
                node.kind,
                resolve::ModuleKind::Entry | resolve::ModuleKind::NativeShell
            )
        })
        .map(|node| {
            let is_package = matches!(
                node.kind,
                resolve::ModuleKind::RegularPackage | resolve::ModuleKind::NamespacePackage
            );
            let package = if is_package {
                node.name.as_str().to_owned()
            } else {
                node.name
                    .as_str()
                    .rsplit_once('.')
                    .map_or_else(String::new, |(parent, _)| parent.to_owned())
            };
            (
                node.name.as_str().to_owned(),
                codegen::ModuleInitializer {
                    symbol: node
                        .source
                        .as_ref()
                        .map(|_| module_symbol(node.name.as_str())),
                    filename: node
                        .source
                        .as_ref()
                        .map_or_else(String::new, |path| path.display().to_string()),
                    package,
                    is_package,
                    search_locations: node
                        .search_locations
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect(),
                    resources: locked_inputs
                        .resources
                        .iter()
                        .filter(|resource| resource.module == node.name.as_str())
                        .map(|resource| codegen::ModuleResource {
                            name: resource.name.clone(),
                            bytes: resource.bytes.clone(),
                        })
                        .collect(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let build_manifest = manifest::render_build_manifest(
        &request.target,
        request.profile,
        &request.capabilities,
        &resolved.graph,
        &locked_inputs,
    );
    let mut artifact = build_project_mir_with_progress(
        &request,
        &mir,
        &module_mir,
        &module_descriptors,
        linked_async_backend,
        report,
    )?;
    let manifest_path = artifact.cache_dir.join("build-manifest.toml");
    // Independent builds can share a cache inside one compiler process.
    // A PID alone lets one publisher rename another publisher's temporary file.
    static MANIFEST_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = MANIFEST_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temporary_manifest = artifact.cache_dir.join(format!(
        ".build-manifest-{}-{sequence}.tmp",
        std::process::id()
    ));
    fs::write(&temporary_manifest, build_manifest)
        .and_then(|()| fs::rename(&temporary_manifest, &manifest_path))
        .map_err(|cause| error("RIM-OUTPUT-001", cause.to_string(), &request.entry))?;
    artifact.manifest = Some(manifest_path);
    Ok(artifact)
}

#[derive(Debug)]
struct NativeModule {
    program: mir::Program,
    source_hash: u64,
}

pub fn build_mir(
    request: &BuildRequest,
    mir: &mir::Program,
) -> Result<BuildArtifact, DiagnosticSet> {
    build_project_mir_with_progress(
        request,
        mir,
        &BTreeMap::new(),
        &BTreeMap::new(),
        None,
        |_| {},
    )
}

fn build_project_mir_with_progress(
    request: &BuildRequest,
    mir: &mir::Program,
    modules: &BTreeMap<String, NativeModule>,
    module_descriptors: &BTreeMap<String, codegen::ModuleInitializer>,
    linked_async_backend: Option<AsyncBackend>,
    mut report: impl FnMut(BuildProgress),
) -> Result<BuildArtifact, DiagnosticSet> {
    mir::verify(mir).map_err(|cause| error("RIM-IR-001", cause, &request.entry))?;
    for module in modules.values() {
        mir::verify(&module.program)
            .map_err(|cause| error("RIM-IR-001", cause, Path::new(&module.program.filename)))?;
    }
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
    let object_bytes = codegen::emit_entry_object(
        &lir,
        request.profile == core::BuildProfile::Release,
        request.heap_limit_bytes,
        module_descriptors,
        linked_async_backend,
        request.capabilities.dynamic_compilation(),
    )
    .map_err(|cause| error("RIM-CODEGEN-001", cause, &request.entry))?;
    let object = object_path(request, &cache_dir);
    fs::create_dir_all(object.parent().expect("object cache path has a parent"))
        .map_err(|cause| error("RIM-OUTPUT-001", cause.to_string(), &request.entry))?;
    fs::write(&object, &object_bytes)
        .map_err(|cause| error("RIM-OUTPUT-001", cause.to_string(), &request.entry))?;
    let mut module_objects = Vec::new();
    let mut total_object_bytes = object_bytes.len() as u64;
    let mut total_functions = mir.functions.len();
    for (name, native_module) in modules {
        let lir = lir::select(&native_module.program);
        let bytes = codegen::emit_module_object(
            &lir,
            request.profile == core::BuildProfile::Release,
            module_descriptors,
            &module_symbol(name),
        )
        .map_err(|cause| {
            error(
                "RIM-CODEGEN-001",
                cause,
                Path::new(&native_module.program.filename),
            )
        })?;
        let path = module_object_path(request, &cache_dir, name, native_module.source_hash);
        fs::write(&path, &bytes).map_err(|cause| {
            error(
                "RIM-OUTPUT-001",
                cause.to_string(),
                Path::new(&native_module.program.filename),
            )
        })?;
        total_object_bytes = total_object_bytes.saturating_add(bytes.len() as u64);
        total_functions = total_functions.saturating_add(native_module.program.functions.len());
        module_objects.push(path);
    }
    report(BuildProgress::Measured(ProgressMeasure {
        stage: BuildStage::EmitObject,
        completed: (module_objects.len() + 1) as u64,
        total: Some((module_objects.len() + 1) as u64),
        unit: "objects",
        detail: format!(
            "{} · {} native functions",
            format_bytes(total_object_bytes),
            format_number(total_functions as u64)
        ),
    }));
    report(BuildProgress::Finished(BuildStage::EmitObject));
    trace(
        &mut report,
        request.debug,
        format!("object: {}", object.display()),
    );

    let runtime = if request.capabilities.dynamic_compilation() {
        compiler_archive(request.profile)
    } else {
        runtime_archive(request.profile)
    }
    .ok_or_else(|| {
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
    let link_objects = std::iter::once(object.as_path())
        .chain(module_objects.iter().map(PathBuf::as_path))
        .collect::<Vec<_>>();
    if let Err(cause) =
        link::link_objects(&link_objects, &runtime, &request.output, request.profile)
    {
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
        manifest: None,
        async_backend: linked_async_backend,
    })
}

fn compiler_archive(profile: core::BuildProfile) -> Option<PathBuf> {
    let path = std::env::var_os("RIMERA_COMPILER_ARCHIVE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target")
                .join(if profile == core::BuildProfile::Release {
                    "release"
                } else {
                    "debug"
                })
                .join("librimera_compiler.a")
        });
    path.is_file().then_some(path)
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

fn module_object_path(
    request: &BuildRequest,
    cache_dir: &Path,
    name: &str,
    source_hash: u64,
) -> PathBuf {
    cache_dir.join("objects").join(format!(
        "{}-{:016x}-{:016x}.o",
        cache_key(request),
        fnv1a_64(name),
        source_hash
    ))
}

fn module_symbol(name: &str) -> String {
    format!("rimera_module_{:016x}", fnv1a_64(name))
}

fn debug_ir_path(request: &BuildRequest, cache_dir: &Path) -> PathBuf {
    cache_dir
        .join("ir")
        .join(format!("{}.ir.py", cache_key(request)))
}

fn cache_key(request: &BuildRequest) -> String {
    let entry_hash = fs::read(&request.entry)
        .map(|source| resolve::source_hash(&source))
        .unwrap_or(0);
    let module_roots = request
        .module_search_roots
        .iter()
        .map(|root| root.display().to_string())
        .collect::<Vec<_>>()
        .join("\0");
    let lock_hash = fs::read(request.project_root.join("rimera.lock"))
        .map(|bytes| resolve::source_hash(&bytes))
        .unwrap_or(0);
    let key = format!(
        "{}\0{}\0{}\0{:?}\0{}\0{entry_hash:016x}\0{module_roots}\0{lock_hash:016x}",
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
