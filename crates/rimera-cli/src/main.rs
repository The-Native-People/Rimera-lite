use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, ExitCode};
use std::time::Instant;

use clap::{Parser, Subcommand, ValueEnum};
use rimera_compiler::BuildProgress;
use rimera_compiler::core::{BuildProfile, Diagnostic, DiagnosticSet, TargetTriple};
use rimera_compiler::project::{
    AsyncBackend, BuildRequest, CAPABILITY_DYNAMIC_COMPILATION, CapabilitySet,
};

#[derive(Debug, Parser)]
#[command(
    name = "rimera-lite",
    version,
    about = "Rimera Lite native Python compiler"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Compile one Python entry point to a native executable.
    Build {
        entry: PathBuf,
        /// Write the executable here. Required unless `--run` is used.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Run the freshly compiled native executable.
        #[arg(long)]
        run: bool,
        #[arg(long)]
        target: Option<String>,
        #[arg(long, value_enum)]
        profile: Option<Profile>,
        /// Write a readable MIR dump and print compiler trace details.
        #[arg(long)]
        debug: bool,
        /// Select the local async executor backend for source-level root execution.
        #[arg(long = "async", value_enum)]
        async_backend: Option<AsyncBackendOption>,
        /// Limit managed heap memory for the generated executable.
        #[arg(long, value_name = "BYTES")]
        heap_limit_bytes: Option<u64>,
        /// Include the native compile/eval/exec service (disabled by default).
        #[arg(long)]
        dynamic_compilation: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
enum Profile {
    #[default]
    Debug,
    Release,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
enum AsyncBackendOption {
    #[default]
    Auto,
    Compio,
    Monoio,
    Tokio,
}

impl From<AsyncBackendOption> for AsyncBackend {
    fn from(value: AsyncBackendOption) -> Self {
        match value {
            AsyncBackendOption::Auto => Self::Auto,
            AsyncBackendOption::Compio => Self::Compio,
            AsyncBackendOption::Monoio => Self::Monoio,
            AsyncBackendOption::Tokio => Self::Tokio,
        }
    }
}

fn main() -> ExitCode {
    let color = match color_enabled() {
        Ok(color) => color,
        Err(message) => {
            eprintln!("{}", format_message_error(&message, false));
            return ExitCode::from(2);
        }
    };

    match execute(parse_cli(), color) {
        Ok(code) => code,
        Err(failure) => {
            eprint!("{}", format_failure(&failure, color));
            ExitCode::from(2)
        }
    }
}

fn parse_cli() -> Cli {
    Cli::parse_from(normalize_arguments(std::env::args_os()))
}

fn normalize_arguments(arguments: impl IntoIterator<Item = OsString>) -> Vec<OsString> {
    let mut arguments = arguments.into_iter().collect::<Vec<_>>();
    let Some(first) = arguments.get(1) else {
        return arguments;
    };
    let is_subcommand = matches!(first.to_str(), Some("build"));
    let is_option = first.to_string_lossy().starts_with('-');
    if !is_subcommand && !is_option {
        arguments.insert(1, OsString::from("build"));
    }
    arguments
}

#[derive(Debug)]
enum CliFailure {
    Message(String),
    Diagnostics(DiagnosticSet),
}

fn resolve_async_backend(
    cli: Option<AsyncBackendOption>,
    configured: Option<AsyncBackendOption>,
) -> AsyncBackend {
    cli.or(configured).unwrap_or_default().into()
}

fn async_backend_metadata(backend: AsyncBackend) -> String {
    format!("async   {} · experimental", backend.as_str())
}

fn execute(cli: Cli, color: bool) -> Result<ExitCode, CliFailure> {
    match cli.command {
        Command::Build {
            entry,
            output,
            run,
            target,
            profile,
            debug,
            async_backend,
            heap_limit_bytes,
            dynamic_compilation,
        } => {
            let project_root = find_project_root(&entry);
            let config = load_config(&project_root)?;
            let target = TargetTriple::parse(
                target
                    .or(config.target)
                    .unwrap_or_else(|| TargetTriple::MACOS_ARM64.to_owned()),
            )
            .map_err(CliFailure::Message)?;
            let profile = match profile.or(config.profile).unwrap_or_default() {
                Profile::Debug => BuildProfile::Debug,
                Profile::Release => BuildProfile::Release,
            };
            let debug = debug || config.debug.unwrap_or(false);
            let run = run || config.run.unwrap_or(false);
            let dynamic_compilation =
                dynamic_compilation || config.dynamic_compilation.unwrap_or(false);
            let async_backend = resolve_async_backend(async_backend, config.async_backend);
            let output = output
                .or(config.output.map(|path| if path.is_absolute() { path } else { project_root.join(path) }))
                .or_else(|| run.then(|| run_output_path(&entry)))
                .ok_or_else(|| CliFailure::Message("an output path is required unless `--run` or `tool.rimera.run = true` is set".to_owned()))?;
            let mut progress = ProgressRenderer::new(color);
            progress.begin(&entry, &output, &target, profile, debug);
            let started = Instant::now();
            let result = rimera_compiler::build_with_progress(
                BuildRequest {
                    project_root,
                    module_search_roots: config.module_roots,
                    entry,
                    output,
                    target,
                    profile,
                    capabilities: CapabilitySet::from_names(
                        dynamic_compilation.then(|| CAPABILITY_DYNAMIC_COMPILATION.to_owned()),
                    ),
                    debug,
                    heap_limit_bytes: heap_limit_bytes.or(config.heap_limit_bytes),
                    async_backend,
                },
                |event| progress.render(event),
            );
            let artifact = match result {
                Ok(artifact) => artifact,
                Err(diagnostics) => {
                    progress.fail();
                    return Err(CliFailure::Diagnostics(diagnostics));
                }
            };
            progress.finish(&artifact, started.elapsed().as_secs_f32());
            if run {
                run_artifact(&artifact.executable, color)
            } else {
                Ok(ExitCode::SUCCESS)
            }
        }
    }
}

fn run_output_path(entry: &Path) -> PathBuf {
    let parent = entry.parent().unwrap_or_else(|| Path::new("."));
    let name = entry
        .file_stem()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| std::ffi::OsStr::new("app"));
    parent.join(".rimera").join("bin").join(name)
}

fn find_project_root(entry: &Path) -> PathBuf {
    let start = entry.parent().unwrap_or_else(|| Path::new("."));
    let mut current = fs::canonicalize(start).unwrap_or_else(|_| start.to_path_buf());
    loop {
        if current.join("pyproject.toml").is_file() {
            return current;
        }
        if !current.pop() {
            return start.to_path_buf();
        }
    }
}

fn run_artifact(executable: &Path, color: bool) -> Result<ExitCode, CliFailure> {
    if color {
        eprintln!(
            "\n\x1b[1;36mRunning [▹]\x1b[0m  \x1b[1m{}\x1b[0m",
            executable.display()
        );
    } else {
        eprintln!("\nRunning [▹]   {}", executable.display());
    }
    let status = ProcessCommand::new(executable).status().map_err(|error| {
        CliFailure::Message(format!("failed to run native executable: {error}"))
    })?;
    Ok(ExitCode::from(status.code().unwrap_or(1) as u8))
}

struct ProgressRenderer {
    color: bool,
    live_updates: bool,
    last_measurement: Option<rimera_compiler::ProgressMeasure>,
    has_live_row: bool,
}

#[derive(Debug, Default)]
struct RimeraConfig {
    output: Option<PathBuf>,
    target: Option<String>,
    profile: Option<Profile>,
    debug: Option<bool>,
    heap_limit_bytes: Option<u64>,
    async_backend: Option<AsyncBackendOption>,
    dynamic_compilation: Option<bool>,
    run: Option<bool>,
    module_roots: Vec<PathBuf>,
}

fn load_config(project_root: &Path) -> Result<RimeraConfig, CliFailure> {
    let path = project_root.join("pyproject.toml");
    let Ok(source) = fs::read_to_string(&path) else {
        return Ok(RimeraConfig::default());
    };
    let document = source.parse::<toml::Table>().map_err(|error| {
        CliFailure::Message(format!("failed to parse {}: {error}", path.display()))
    })?;
    let Some(tool) = document.get("tool").and_then(toml::Value::as_table) else {
        return Ok(RimeraConfig::default());
    };
    let Some(rimera) = tool.get("rimera").and_then(toml::Value::as_table) else {
        return Ok(RimeraConfig::default());
    };
    let output = rimera
        .get("output")
        .and_then(toml::Value::as_str)
        .map(PathBuf::from);
    let target = rimera
        .get("target")
        .and_then(toml::Value::as_str)
        .map(str::to_owned);
    let profile = rimera
        .get("profile")
        .and_then(toml::Value::as_str)
        .map(|value| match value {
            "debug" => Ok(Profile::Debug),
            "release" => Ok(Profile::Release),
            _ => Err(CliFailure::Message(format!(
                "tool.rimera.profile must be `debug` or `release`, not `{value}`"
            ))),
        })
        .transpose()?;
    let debug = rimera.get("debug").and_then(toml::Value::as_bool);
    let async_backend = rimera
        .get("async")
        .and_then(toml::Value::as_str)
        .map(|value| match value {
            "auto" => Ok(AsyncBackendOption::Auto),
            "compio" => Ok(AsyncBackendOption::Compio),
            "monoio" => Ok(AsyncBackendOption::Monoio),
            "tokio" => Ok(AsyncBackendOption::Tokio),
            _ => Err(CliFailure::Message(format!(
                "tool.rimera.async must be `auto`, `compio`, `monoio`, or `tokio`, not `{value}`"
            ))),
        })
        .transpose()?;
    let dynamic_compilation = rimera
        .get("dynamic_compilation")
        .and_then(toml::Value::as_bool);
    let heap_limit_bytes = rimera
        .get("heap_limit_bytes")
        .and_then(toml::Value::as_integer)
        .map(|value| {
            u64::try_from(value).map_err(|_| {
                CliFailure::Message("tool.rimera.heap_limit_bytes must be non-negative".to_owned())
            })
        })
        .transpose()?;
    let run = rimera.get("run").and_then(toml::Value::as_bool);
    let module_roots = rimera
        .get("module_roots")
        .map(|value| {
            let values = value.as_array().ok_or_else(|| {
                CliFailure::Message("tool.rimera.module_roots must be an array of paths".to_owned())
            })?;
            values
                .iter()
                .map(|value| {
                    value.as_str().map(PathBuf::from).ok_or_else(|| {
                        CliFailure::Message(
                            "every tool.rimera.module_roots entry must be a path string".to_owned(),
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    Ok(RimeraConfig {
        output,
        target,
        profile,
        debug,
        heap_limit_bytes,
        async_backend,
        dynamic_compilation,
        run,
        module_roots,
    })
}

impl ProgressRenderer {
    fn new(color: bool) -> Self {
        Self {
            color,
            live_updates: std::io::stderr().is_terminal(),
            last_measurement: None,
            has_live_row: false,
        }
    }

    fn begin(
        &self,
        entry: &Path,
        output: &Path,
        target: &TargetTriple,
        profile: BuildProfile,
        debug: bool,
    ) {
        let entry = display_path(entry);
        let output = display_path(output);
        let profile = match profile {
            BuildProfile::Debug => "debug",
            BuildProfile::Release => "release",
        };
        let mode = if debug { " · trace" } else { "" };
        if self.color {
            eprintln!("\x1b[1;36mRimera Lite [♣]\x1b[0m");
            eprintln!("  \x1b[2m|-+ source\x1b[0m  \x1b[1m{entry}\x1b[0m");
            eprintln!(
                "  \x1b[2m|-+ target\x1b[0m  \x1b[1m{}\x1b[0m \x1b[2m· {profile}{mode}\x1b[0m",
                target.as_str()
            );
            eprintln!("  \x1b[2m|-+ output\x1b[0m  \x1b[1m{output}\x1b[0m");
            eprintln!("  \x1b[2m|-+ linker\x1b[0m  \x1b[1mclang\x1b[0m");
            eprintln!();
        } else {
            eprintln!("Rimera Lite [♣]");
            eprintln!("  |-+ source  {entry}");
            eprintln!("  |-+ target  {} · {profile}{mode}", target.as_str());
            eprintln!("  |-+ output  {output}");
            eprintln!("  |-+ linker  clang");
            eprintln!();
        }
    }

    fn render(&mut self, event: BuildProgress) {
        match event {
            BuildProgress::Started(stage) => {
                self.last_measurement = None;
                if self.live_updates {
                    self.render_live(stage, None, false);
                } else {
                    eprintln!("  ○  {}/5  {}", stage_step(stage), stage_title(stage));
                }
            }
            BuildProgress::Finished(stage) => {
                let measurement = self
                    .last_measurement
                    .as_ref()
                    .filter(|measurement| measurement.stage == stage)
                    .cloned();
                if self.live_updates {
                    self.render_live(stage, measurement.as_ref(), true);
                    eprintln!();
                } else {
                    eprintln!("{}", self.stage_line(stage, measurement.as_ref(), true));
                }
                self.last_measurement = None;
                self.has_live_row = false;
            }
            BuildProgress::Measured(measurement) => self.render_measurement(measurement),
            BuildProgress::Trace(message) => {
                self.clear_live_row();
                if self.color {
                    eprintln!("  \x1b[2m│\x1b[0m \x1b[35mdebug\x1b[0m \x1b[2m{message}\x1b[0m");
                } else {
                    eprintln!("  │ debug {message}");
                }
            }
        }
    }

    fn render_measurement(&mut self, measurement: rimera_compiler::ProgressMeasure) {
        let stage = measurement.stage;
        self.last_measurement = Some(measurement);
        if self.live_updates {
            let measurement = self.last_measurement.clone();
            self.render_live(stage, measurement.as_ref(), false);
        }
    }

    fn render_live(
        &mut self,
        stage: rimera_compiler::BuildStage,
        measurement: Option<&rimera_compiler::ProgressMeasure>,
        completed: bool,
    ) {
        eprint!(
            "\r\x1b[2K{}",
            self.stage_line(stage, measurement, completed)
        );
        self.has_live_row = true;
    }

    fn clear_live_row(&mut self) {
        if self.live_updates && self.has_live_row {
            eprint!("\r\x1b[2K");
            self.has_live_row = false;
        }
    }

    fn stage_line(
        &self,
        stage: rimera_compiler::BuildStage,
        measurement: Option<&rimera_compiler::ProgressMeasure>,
        completed: bool,
    ) -> String {
        let percent = measurement.map_or(0, measurement_percent);
        let marker = if completed { "✓" } else { "○" };
        let metric = measurement.map_or_else(|| "preparing…".to_owned(), measurement_summary);
        let line = format!(
            "  {marker}  {}/5  {:<18} {} {percent:>3}%  {metric}",
            stage_step(stage),
            stage_title(stage),
            progress_bar(percent),
        );
        if !self.color {
            return line;
        }
        let accent = if completed {
            "\x1b[1;32m"
        } else {
            "\x1b[1;36m"
        };
        format!(
            "  {accent}{marker}\x1b[0m  \x1b[2m{}/5\x1b[0m  \x1b[1m{:<18}\x1b[0m {accent}{} {percent:>3}%\x1b[0m  \x1b[2m{metric}\x1b[0m",
            stage_step(stage),
            stage_title(stage),
            progress_bar(percent),
        )
    }

    fn fail(&mut self) {
        self.clear_live_row();
        if self.color {
            eprintln!("\x1b[1;31m✗  Build failed\x1b[0m\n");
        } else {
            eprintln!("✗  Build failed\n");
        }
    }

    fn finish(&mut self, artifact: &rimera_compiler::project::BuildArtifact, elapsed_seconds: f32) {
        self.clear_live_row();
        let bytes = std::fs::metadata(&artifact.executable)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        if self.color {
            eprintln!();
            if let Some(backend) = artifact.async_backend {
                eprintln!(
                    "  \x1b[2m|-+ async\x1b[0m   \x1b[1m{}\x1b[0m \x1b[2m· experimental\x1b[0m",
                    backend.as_str()
                );
            }
            if let Some(ir_dump) = &artifact.ir_dump {
                eprintln!("\x1b[1;35mDebug:\x1b[0m");
                eprintln!("  \x1b[2m-+ cache\x1b[0m  {}", artifact.cache_dir.display());
                eprintln!("  \x1b[2m-+ ir\x1b[0m     {}", ir_dump.display());
                eprintln!(
                    "  \x1b[2m-+ ir\x1b[0m     {}",
                    artifact.executable.display()
                );
            }
            //eprintln!(
            //    "\x1b[1;32m[✔] Build complete \x1b[0m @ \x1b[1m{}\x1b[0m",
            //   artifact.executable.display()
            //);
            eprintln!(
                "\n\x1b[1mDone in {elapsed_seconds:.2}s\x1b[0m \x1b[2m+|+\x1b[0m \x1b[1mSize:\x1b[0m {bytes} bytes."
            );
        } else {
            if let Some(backend) = artifact.async_backend {
                eprintln!("  |-+ {}", async_backend_metadata(backend));
            }
            if let Some(ir_dump) = &artifact.ir_dump {
                eprintln!("Debug:");
                eprintln!("  -+ cache  {}", artifact.cache_dir.display());
                eprintln!("  -+ ir     {}", ir_dump.display());
            }
            //eprintln!("[✔] Build complete @  {}", artifact.executable.display());
            eprintln!("\nDone in {elapsed_seconds:.2}s +|+ Size: {bytes} bytes.");
        }
    }
}

fn stage_step(stage: rimera_compiler::BuildStage) -> u8 {
    match stage {
        rimera_compiler::BuildStage::AnalyzeSource => 1,
        rimera_compiler::BuildStage::AnalyzeSemantics => 2,
        rimera_compiler::BuildStage::LowerMir => 3,
        rimera_compiler::BuildStage::EmitObject => 4,
        rimera_compiler::BuildStage::LinkRuntime => 5,
    }
}

fn stage_title(stage: rimera_compiler::BuildStage) -> &'static str {
    match stage {
        rimera_compiler::BuildStage::AnalyzeSource => "Read source",
        rimera_compiler::BuildStage::AnalyzeSemantics => "Analyze semantics",
        rimera_compiler::BuildStage::LowerMir => "Lower native IR",
        rimera_compiler::BuildStage::EmitObject => "Emit object",
        rimera_compiler::BuildStage::LinkRuntime => "Link with clang",
    }
}

fn measurement_percent(measurement: &rimera_compiler::ProgressMeasure) -> u8 {
    measurement
        .total
        .filter(|total| *total > 0)
        .map_or(0, |total| {
            (measurement.completed.saturating_mul(100) / total).min(100) as u8
        })
}

fn measurement_summary(measurement: &rimera_compiler::ProgressMeasure) -> String {
    let completed = format_count(measurement.completed, measurement.unit);
    let metric = measurement.total.map_or(completed.clone(), |total| {
        format!("{completed} / {}", format_count(total, measurement.unit))
    });
    if measurement.detail.is_empty() {
        metric
    } else {
        format!("{metric}  ·  {}", measurement.detail)
    }
}

fn format_count(value: u64, unit: &str) -> String {
    if unit == "source bytes" {
        format_bytes(value)
    } else if value == 1 {
        format!("1 {}", singular_unit(unit))
    } else {
        format!("{} {unit}", format_number(value))
    }
}

fn singular_unit(unit: &str) -> &str {
    unit.strip_suffix('s').unwrap_or(unit)
}

fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    if bytes >= MIB {
        format!("{:.2} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.2} KiB", bytes as f64 / KIB as f64)
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

fn progress_bar(percent: u8) -> String {
    const WIDTH: usize = 9;
    let percent = percent.min(100);
    let filled = if percent == 0 {
        1
    } else {
        ((usize::from(percent) * (WIDTH - 1)).div_ceil(100)).max(1)
    };
    if percent == 100 {
        format!("[{}]", "=".repeat(WIDTH))
    } else {
        format!(
            "[{}>{}]",
            "=".repeat(filled),
            " ".repeat(WIDTH - filled - 1)
        )
    }
}

fn format_failure(failure: &CliFailure, color: bool) -> String {
    match failure {
        CliFailure::Message(message) => format_message_error(message, color),
        CliFailure::Diagnostics(diagnostics) => format_diagnostics(diagnostics, color),
    }
}

fn format_message_error(message: &str, color: bool) -> String {
    if color {
        format!("\x1b[1;31merror\x1b[0m\n  \x1b[1m{message}\x1b[0m\n")
    } else {
        format!("error\n  {message}\n")
    }
}

fn format_diagnostics(diagnostics: &DiagnosticSet, color: bool) -> String {
    let mut output = String::new();
    for (index, diagnostic) in diagnostics.as_slice().iter().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        format_diagnostic(&mut output, diagnostic, color);
    }
    output
}

fn format_diagnostic(output: &mut String, diagnostic: &Diagnostic, color: bool) {
    if color {
        let _ = writeln!(
            output,
            "\x1b[1;31merror\x1b[0m\x1b[1m[{}]\x1b[0m",
            diagnostic.code
        );
        for line in wrap_message(&diagnostic.message, 82) {
            let _ = writeln!(output, "  \x1b[1m{line}\x1b[0m");
        }
    } else {
        let _ = writeln!(output, "error[{}]", diagnostic.code);
        for line in wrap_message(&diagnostic.message, 82) {
            let _ = writeln!(output, "  {line}");
        }
    }

    let path = display_path(&diagnostic.path);
    if let Some(excerpt) = source_excerpt(diagnostic) {
        let location = format!("{path}:{}:{}", excerpt.line_number, excerpt.column);
        let line_number = excerpt.line_number.to_string();
        let gutter_width = line_number.len();
        let caret_padding = " ".repeat(excerpt.column.saturating_sub(1));
        let carets = "^".repeat(excerpt.width.max(1));
        if color {
            let _ = writeln!(output, "  \x1b[2;34m╭─\x1b[0m \x1b[1;36m{location}\x1b[0m");
            let _ = writeln!(
                output,
                "\x1b[2;34m{line_number:>gutter_width$} │\x1b[0m {}",
                excerpt.text
            );
            let _ = writeln!(
                output,
                "\x1b[2;34m{:>gutter_width$} │\x1b[0m {caret_padding}\x1b[1;31m{carets}\x1b[0m",
                ""
            );
            let _ = writeln!(output, "  \x1b[2;34m╰─\x1b[0m");
        } else {
            let _ = writeln!(output, "  ╭─ {location}");
            let _ = writeln!(output, "{line_number:>gutter_width$} │ {}", excerpt.text);
            let _ = writeln!(output, "{:>gutter_width$} │ {caret_padding}{carets}", "");
            let _ = writeln!(output, "  ╰─");
        }
    } else if color {
        let _ = writeln!(
            output,
            "  \x1b[2;34m╰─\x1b[0m \x1b[1;36m{path}:{}..{}\x1b[0m",
            diagnostic.span.start, diagnostic.span.end
        );
    } else {
        let _ = writeln!(
            output,
            "  ╰─ {path}:{}..{}",
            diagnostic.span.start, diagnostic.span.end
        );
    }
}

struct SourceExcerpt {
    line_number: usize,
    column: usize,
    width: usize,
    text: String,
}

fn source_excerpt(diagnostic: &Diagnostic) -> Option<SourceExcerpt> {
    let source = fs::read_to_string(&diagnostic.path).ok()?;
    let start = floor_char_boundary(&source, diagnostic.span.start as usize);
    let end = floor_char_boundary(&source, diagnostic.span.end as usize).max(start);
    let line_start = source[..start]
        .rfind('\n')
        .map_or(0, |position| position + 1);
    let line_end = source[start..]
        .find('\n')
        .map_or(source.len(), |position| start + position);
    let line_number = source[..line_start]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    let column = source[line_start..start].chars().count() + 1;
    let highlighted_end = end.min(line_end);
    let width = source[start..highlighted_end].chars().count().max(1);
    Some(SourceExcerpt {
        line_number,
        column,
        width,
        text: source[line_start..line_end].to_owned(),
    })
}

fn floor_char_boundary(source: &str, offset: usize) -> usize {
    let mut offset = offset.min(source.len());
    while !source.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn display_path(path: &Path) -> String {
    std::env::current_dir()
        .ok()
        .and_then(|root| path.strip_prefix(root).ok().map(Path::to_path_buf))
        .unwrap_or_else(|| path.to_path_buf())
        .display()
        .to_string()
}

fn wrap_message(message: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in message.lines() {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            if !current.is_empty() && current.len() + word.len() + 1 > width {
                lines.push(std::mem::take(&mut current));
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn color_enabled() -> Result<bool, String> {
    match std::env::var("RIMERA_COLOR").as_deref() {
        Ok("always") => Ok(true),
        Ok("never") => Ok(false),
        Ok("auto") | Err(std::env::VarError::NotPresent) => Ok(std::io::stderr().is_terminal()),
        Ok(value) => Err(format!(
            "RIMERA_COLOR must be `auto`, `always`, or `never`, not `{value}`"
        )),
        Err(error) => Err(format!("failed to read RIMERA_COLOR: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rimera_compiler::core::Span;

    fn fixture_diagnostic() -> DiagnosticSet {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/basic/hello.py");
        DiagnosticSet::one(Diagnostic::new(
            "RIM-TEST-001",
            "a deliberately long diagnostic message that verifies readable wrapping without losing the original words",
            path,
            Span::new(0, 5),
        ))
    }

    #[test]
    fn plain_diagnostics_include_source_location_and_caret() {
        let rendered = format_diagnostics(&fixture_diagnostic(), false);

        assert!(rendered.contains("error[RIM-TEST-001]"));
        assert!(rendered.contains("1 │ print(\"hello\")"));
        assert!(rendered.contains("│ ^^^^^"));
        assert!(!rendered.contains("\x1b["));
    }

    #[test]
    fn colored_diagnostics_emphasize_error_location_and_source() {
        let rendered = format_diagnostics(&fixture_diagnostic(), true);

        assert!(rendered.contains("\x1b[1;31merror\x1b[0m"));
        assert!(rendered.contains("\x1b[1;36m"));
        assert!(rendered.contains("\x1b[1;31m^^^^^\x1b[0m"));
    }

    #[test]
    fn paths_under_the_workspace_are_compact() {
        let root = std::env::current_dir().expect("current directory should be available");
        let path = root.join("tests/fixtures/basic/hello.py");

        assert_eq!(display_path(&path), "tests/fixtures/basic/hello.py");
    }

    #[test]
    fn progress_rendering_uses_measured_counts_and_stage_order() {
        assert_eq!(stage_step(rimera_compiler::BuildStage::AnalyzeSource), 1);
        assert_eq!(stage_step(rimera_compiler::BuildStage::LinkRuntime), 5);
        assert_eq!(
            stage_title(rimera_compiler::BuildStage::AnalyzeSource),
            "Read source"
        );
        assert_eq!(format_count(1, "native functions"), "1 native function");
        assert_eq!(
            format_count(39_369, "native functions"),
            "39,369 native functions"
        );
        assert_eq!(format_bytes(1_536), "1.50 KiB");
        assert_eq!(progress_bar(8), "[=>       ]");
        assert_eq!(progress_bar(80), "[=======> ]");
        assert_eq!(progress_bar(100), "[=========]");

        let measurement = rimera_compiler::ProgressMeasure {
            stage: rimera_compiler::BuildStage::AnalyzeSource,
            completed: 64,
            total: Some(726),
            unit: "source bytes",
            detail: "read".to_owned(),
        };
        assert_eq!(measurement_percent(&measurement), 8);
        assert_eq!(measurement_summary(&measurement), "64 B / 726 B  ·  read");
    }

    #[test]
    fn build_command_accepts_a_managed_heap_limit() {
        let cli = Cli::try_parse_from([
            "rimera",
            "build",
            "main.py",
            "--output",
            "app",
            "--heap-limit-bytes",
            "1048576",
        ])
        .unwrap();
        let Command::Build {
            heap_limit_bytes, ..
        } = cli.command;
        assert_eq!(heap_limit_bytes, Some(1_048_576));
    }

    #[test]
    fn build_command_accepts_dynamic_compilation_capability() {
        let cli = Cli::try_parse_from([
            "rimera",
            "build",
            "main.py",
            "--output",
            "app",
            "--dynamic-compilation",
        ])
        .unwrap();
        let Command::Build {
            dynamic_compilation,
            ..
        } = cli.command;
        assert!(dynamic_compilation);
    }

    #[test]
    fn async_backend_cli_overrides_project_configuration() {
        assert_eq!(
            resolve_async_backend(
                Some(AsyncBackendOption::Tokio),
                Some(AsyncBackendOption::Compio),
            ),
            AsyncBackend::Tokio
        );
        assert_eq!(
            resolve_async_backend(None, Some(AsyncBackendOption::Compio)),
            AsyncBackend::Compio
        );
        assert_eq!(resolve_async_backend(None, None), AsyncBackend::Auto);
        assert_eq!(
            async_backend_metadata(AsyncBackend::Compio),
            "async   compio · experimental"
        );
    }

    #[test]
    fn build_command_accepts_async_backend_selection() {
        let cli = Cli::try_parse_from([
            "rimera", "build", "main.py", "--output", "app", "--async", "tokio",
        ])
        .unwrap();
        let Command::Build { async_backend, .. } = cli.command;
        assert_eq!(async_backend, Some(AsyncBackendOption::Tokio));
    }

    #[test]
    fn bare_source_entry_normalizes_to_the_build_command() {
        let cli = Cli::try_parse_from(normalize_arguments([
            OsString::from("rimera"),
            OsString::from("main.py"),
            OsString::from("--run"),
        ]))
        .unwrap();
        let Command::Build {
            entry, output, run, ..
        } = cli.command;
        assert_eq!(entry, PathBuf::from("main.py"));
        assert!(output.is_none());
        assert!(run);
    }

    #[test]
    fn run_output_is_kept_under_the_project_cache() {
        assert_eq!(
            run_output_path(Path::new("project/main.py")),
            PathBuf::from("project/.rimera/bin/main")
        );
    }

    #[test]
    fn tool_rimera_configuration_is_loaded_from_the_project_root() {
        let root = std::env::temp_dir().join(format!("rimera-config-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("pyproject.toml"),
            "[tool.rimera]\noutput = \"dist/app\"\nprofile = \"release\"\ndebug = true\nasync = \"compio\"\ndynamic_compilation = true\nheap_limit_bytes = 4096\nrun = true\nmodule_roots = [\"src\", \"vendor\"]\n",
        )
        .unwrap();
        let config = load_config(&root).unwrap();
        assert_eq!(config.output, Some(PathBuf::from("dist/app")));
        assert_eq!(config.profile, Some(Profile::Release));
        assert_eq!(config.debug, Some(true));
        assert_eq!(config.async_backend, Some(AsyncBackendOption::Compio));
        assert_eq!(config.dynamic_compilation, Some(true));
        assert_eq!(config.heap_limit_bytes, Some(4096));
        assert_eq!(config.run, Some(true));
        assert_eq!(
            config.module_roots,
            [PathBuf::from("src"), PathBuf::from("vendor")]
        );
        fs::remove_dir_all(root).unwrap();
    }
}
