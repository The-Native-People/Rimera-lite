use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use rimera_compiler::core::{BuildProfile, Span, TargetTriple};
use rimera_compiler::mir::{
    Block, BlockId, Constant, Function, FunctionId, Operation, OperationKind, Program, Terminator,
    ValueId,
};
use rimera_compiler::project::{AsyncBackend, BuildRequest, CapabilitySet};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn gate11_dynamic_namespaces_use_public_native_pipeline() {
    let status = Command::new("cargo")
        .current_dir(workspace())
        .args(["build", "-p", "rimera-compiler", "--lib"])
        .status()
        .unwrap();
    assert!(status.success());
    let mut build_request = request("gate11_dynamic_namespaces.py", output("gate11-dynamic"));
    build_request.capabilities = CapabilitySet::from_names(["dynamic_compilation".to_owned()]);
    let expected = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&build_request.entry)
        .output()
        .unwrap();
    assert!(
        expected.status.success(),
        "{}",
        String::from_utf8_lossy(&expected.stderr)
    );
    let artifact = rimera_compiler::build(build_request).unwrap();
    let actual = run(&artifact.executable);
    assert!(
        actual.status.success(),
        "{}",
        String::from_utf8_lossy(&actual.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&actual.stdout),
        String::from_utf8_lossy(&expected.stdout)
    );
    assert_native_only_artifact(&artifact.executable);
    let mut limited = request(
        "gate11_dynamic_namespaces.py",
        output("gate11-dynamic-small-heap"),
    );
    limited.capabilities = CapabilitySet::from_names(["dynamic_compilation".to_owned()]);
    limited.heap_limit_bytes = Some(262_144);
    let limited = rimera_compiler::build(limited).unwrap();
    let actual = run(&limited.executable);
    assert!(
        actual.status.success(),
        "{}",
        String::from_utf8_lossy(&actual.stderr)
    );
    assert_eq!(actual.stdout, expected.stdout);
    let denied = request("gate11_dynamic_namespaces.py", output("gate11-denied"));
    let denied_output = denied.output.clone();
    let errors = rimera_compiler::build(denied).unwrap_err();
    assert!(
        errors
            .as_slice()
            .iter()
            .any(|error| error.code == "RIM-CAP-G7-02")
    );
    assert!(!denied_output.exists());
}

#[test]
fn gate11_eval_and_exec_scope_conformance() {
    assert_gate11_fixtures(&["gate11_eval_scope.py", "gate11_exec_scope.py"]);
}

#[test]
fn gate11_cache_lifetime_and_dynamic_composition_match_cpython() {
    assert_gate11_fixtures(&["gate11_cache_lifetime.py", "gate11_dynamic_composition.py"]);
}

#[test]
fn gate11_adversarial_limits_and_final_artifact_audit_are_stable() {
    let status = Command::new("cargo")
        .current_dir(workspace())
        .args(["build", "-p", "rimera-compiler", "--lib"])
        .status()
        .unwrap();
    assert!(status.success());

    let mut build_request = request(
        "gate11_adversarial_limits.py",
        output("gate11-adversarial-limits"),
    );
    build_request.capabilities = CapabilitySet::from_names(["dynamic_compilation".to_owned()]);
    build_request.heap_limit_bytes = Some(2_500_000);
    let artifact = rimera_compiler::build(build_request).unwrap();
    let actual = run(&artifact.executable);
    assert!(
        actual.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&actual.stdout),
        String::from_utf8_lossy(&actual.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&actual.stdout),
        concat!(
            "source-limit RuntimeError dynamic source exceeds the 1048576 byte limit\n",
            "source-recovery 42\n",
            "depth-limit RuntimeError dynamic execution depth exceeds the 32 level limit\n",
            "depth-recovery 42\n",
            "syntax SyntaxError invalid syntax. Got unexpected token Newline at byte offset 2\n",
            "syntax-state 42 True False\n",
            "runtime ValueError stop\n",
            "runtime-state 1 False\n",
        )
    );
    assert!(actual.stderr.is_empty());
    assert_native_only_artifact(&artifact.executable);

    let static_artifact =
        rimera_compiler::build(request("hello.py", output("gate11-static-final-audit"))).unwrap();
    let symbols = artifact_symbols(&static_artifact.executable);
    assert!(!symbols.contains("rimera_dynamic_compiler_install"));
    assert!(!symbols.contains("compile_native"));
    assert_native_only_artifact(&static_artifact.executable);
}

fn assert_gate11_fixtures(fixtures: &[&str]) {
    let status = Command::new("cargo")
        .current_dir(workspace())
        .args(["build", "-p", "rimera-compiler", "--lib"])
        .status()
        .unwrap();
    assert!(status.success());
    for fixture in fixtures {
        let mut build_request = request(fixture, output(fixture));
        build_request.capabilities = CapabilitySet::from_names(["dynamic_compilation".to_owned()]);
        build_request.heap_limit_bytes = Some(262_144);
        let expected = Command::new("/opt/homebrew/bin/python3.12")
            .arg(&build_request.entry)
            .output()
            .unwrap();
        assert!(
            expected.status.success(),
            "{fixture}: {}",
            String::from_utf8_lossy(&expected.stderr)
        );
        let artifact = rimera_compiler::build(build_request).unwrap();
        let actual = run(&artifact.executable);
        assert!(
            actual.status.success(),
            "{fixture}: stdout={} stderr={}",
            String::from_utf8_lossy(&actual.stdout),
            String::from_utf8_lossy(&actual.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&actual.stdout),
            String::from_utf8_lossy(&expected.stdout),
            "{fixture}"
        );
        assert_native_only_artifact(&artifact.executable);
    }
}

#[test]
fn gate12_slice2_weak_references_callbacks_hash_equality_and_proxies_match_cpython() {
    let entry = workspace().join("tests/fixtures/basic/gate12_weak_reference_core.py");
    let expected = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&entry)
        .output()
        .unwrap();
    assert!(
        expected.status.success(),
        "{}",
        String::from_utf8_lossy(&expected.stderr)
    );

    let mut build_request = request(
        "gate12_weak_reference_core.py",
        output("gate12-weak-reference-core"),
    );
    build_request.heap_limit_bytes = Some(262_144);
    let artifact = rimera_compiler::build(build_request).unwrap();
    let actual = run(&artifact.executable);
    assert!(
        actual.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&actual.stdout),
        String::from_utf8_lossy(&actual.stderr)
    );
    assert_eq!(actual.stdout, expected.stdout);
    assert!(actual.stderr.is_empty());
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate12_slice3_weak_containers_prune_and_iterate_without_strengthening_referents() {
    let entry = workspace().join("tests/fixtures/basic/gate12_weak_containers.py");
    let expected = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&entry)
        .output()
        .unwrap();
    assert!(
        expected.status.success(),
        "{}",
        String::from_utf8_lossy(&expected.stderr)
    );

    let mut build_request = request(
        "gate12_weak_containers.py",
        output("gate12-weak-containers"),
    );
    build_request.heap_limit_bytes = Some(262_144);
    let artifact = rimera_compiler::build(build_request).unwrap();
    let actual = run(&artifact.executable);
    assert!(
        actual.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&actual.stdout),
        String::from_utf8_lossy(&actual.stderr)
    );
    assert_eq!(actual.stdout, expected.stdout);
    assert!(actual.stderr.is_empty());
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate12_slice4_del_order_unraisable_reporting_and_shutdown_match_cpython() {
    let entry = workspace().join("tests/fixtures/basic/gate12_finalization.py");
    let expected = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&entry)
        .output()
        .unwrap();
    assert!(expected.status.success());

    let mut build_request = request(
        "gate12_finalization.py",
        output("gate12-finalization"),
    );
    build_request.heap_limit_bytes = Some(262_144);
    let artifact = rimera_compiler::build(build_request).unwrap();
    let actual = run(&artifact.executable);
    assert!(
        actual.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&actual.stdout),
        String::from_utf8_lossy(&actual.stderr)
    );
    assert_eq!(actual.stdout, expected.stdout);

    let stderr = String::from_utf8_lossy(&actual.stderr);
    assert_eq!(stderr.matches("Exception ignored in:").count(), 2);
    assert!(stderr.contains("RuntimeError: finalizer boom"));
    assert!(stderr.contains("ValueError: weak callback boom"));
    assert_native_only_artifact(&artifact.executable);
}

fn ensure_runtime_archive() {
    static RUNTIME_ARCHIVE: OnceLock<()> = OnceLock::new();
    RUNTIME_ARCHIVE.get_or_init(|| {
        let status = Command::new("cargo")
            .current_dir(workspace())
            .args([
                "rustc",
                "-p",
                "rimera-runtime",
                "--lib",
                "--crate-type",
                "staticlib",
            ])
            .status()
            .expect("run Cargo to build the runtime static archive");
        assert!(status.success(), "runtime static archive build failed");
    });
}

fn ensure_release_runtime_archive() {
    static RUNTIME_ARCHIVE: OnceLock<()> = OnceLock::new();
    RUNTIME_ARCHIVE.get_or_init(|| {
        let status = Command::new("cargo")
            .current_dir(workspace())
            .args([
                "rustc",
                "-p",
                "rimera-runtime",
                "--release",
                "--lib",
                "--crate-type",
                "staticlib",
            ])
            .status()
            .expect("run Cargo to build the release runtime static archive");
        assert!(
            status.success(),
            "release runtime static archive build failed"
        );
    });
}

fn output(name: &str) -> PathBuf {
    // Different conformance tests intentionally compile the same fixture.
    // Give each request its own output so a concurrent linker cannot replace
    // another test's executable between build completion and process launch.
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!(
        "rimera-native-{}-{sequence}-{name}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).unwrap();
    directory.join(name)
}

fn request(fixture: &str, output: PathBuf) -> BuildRequest {
    ensure_runtime_archive();
    let root = workspace();
    BuildRequest {
        project_root: root.clone(),
        module_search_roots: Vec::new(),
        entry: root.join("tests/fixtures/basic").join(fixture),
        output,
        target: TargetTriple::default(),
        profile: BuildProfile::Debug,
        capabilities: CapabilitySet::default(),
        debug: false,
        heap_limit_bytes: None,
        async_backend: AsyncBackend::Auto,
    }
}

fn relocate_entry_to_project(request: &mut BuildRequest) {
    fs::create_dir_all(&request.project_root).unwrap();
    let entry = request.project_root.join(
        request
            .entry
            .file_name()
            .expect("fixture entry has a file name"),
    );
    fs::copy(&request.entry, &entry).unwrap();
    request.entry = entry;
}

fn isolated_async_request(project_name: &str, fixture: &str, output_name: &str) -> BuildRequest {
    let source = workspace().join("tests/fixtures/async").join(fixture);
    let project_root = std::env::temp_dir().join(format!(
        "rimera-native-{}-{project_name}-project",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&project_root);
    let mut request = request("hello.py", output(output_name));
    request.project_root = project_root;
    request.entry = source;
    relocate_entry_to_project(&mut request);
    request
}

fn run(path: &Path) -> std::process::Output {
    Command::new(path).output().unwrap()
}

fn artifact_symbols(path: &Path) -> String {
    let symbols = Command::new("nm").arg(path).output().unwrap();
    assert!(symbols.status.success());
    String::from_utf8_lossy(&symbols.stdout).into_owned()
}

fn assert_native_only_artifact(path: &Path) {
    let symbols = artifact_symbols(path);
    for forbidden in ["Py_", "PyObject", "setjmp", "longjmp", "rimera_compat"] {
        assert!(
            !symbols.contains(forbidden),
            "artifact contains forbidden symbol `{forbidden}`"
        );
    }
}

fn assert_no_async_backend_symbols(path: &Path) {
    let symbols = Command::new("nm").arg(path).output().unwrap();
    assert!(symbols.status.success());
    let symbols = String::from_utf8_lossy(&symbols.stdout).to_ascii_lowercase();
    for forbidden in ["rimera_async", "compio", "monoio", "tokio"] {
        assert!(
            !symbols.contains(forbidden),
            "synchronous artifact unexpectedly contains async/backend symbol `{forbidden}`"
        );
    }
}

#[test]
fn hand_built_mir_keeps_managed_values_alive_across_collection() {
    let output = output("mir_gc");
    let request = request("hello.py", output.clone());
    let function = Function {
        kind: rimera_compiler::mir::FunctionKind::Module,
        name: "<module>".to_owned(),
        qualified_name: "<module>".to_owned(),
        parameters: vec![],
        native_local_count: 0,
        entry: BlockId(0),
        value_count: 2,
        exception_edges: std::collections::BTreeMap::new(),
        blocks: vec![Block {
            parameters: vec![],
            operations: vec![
                Operation {
                    span: Span::default(),
                    kind: OperationKind::Constant {
                        dest: ValueId(0),
                        value: Constant::String("alive".to_owned()),
                    },
                },
                Operation {
                    span: Span::default(),
                    kind: OperationKind::Constant {
                        dest: ValueId(1),
                        value: Constant::Int("123456789012345678901234567890".to_owned()),
                    },
                },
                Operation {
                    span: Span::default(),
                    kind: OperationKind::Collect,
                },
                Operation {
                    span: Span::default(),
                    kind: OperationKind::Print {
                        values: vec![ValueId(0), ValueId(1)],
                    },
                },
            ],
            terminator: Terminator::Return { code: 0 },
        }],
    };
    let program = Program {
        filename: "<native-test>".to_owned(),
        line_starts: vec![0],
        entry: FunctionId(0),
        functions: vec![function],
    };
    let artifact = rimera_compiler::build_mir(&request, &program).unwrap();
    let result = run(&artifact.executable);
    assert_eq!(
        result.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&result.stdout),
        "alive 123456789012345678901234567890\n"
    );
}

#[test]
fn hand_built_mir_traces_children_through_a_managed_graph() {
    let output = output("mir_gc_graph");
    let request = request("hello.py", output.clone());
    let function = Function {
        kind: rimera_compiler::mir::FunctionKind::Module,
        name: "<module>".to_owned(),
        qualified_name: "<module>".to_owned(),
        parameters: vec![],
        native_local_count: 0,
        entry: BlockId(0),
        value_count: 3,
        exception_edges: std::collections::BTreeMap::new(),
        blocks: vec![Block {
            parameters: vec![],
            operations: vec![
                Operation {
                    span: Span::default(),
                    kind: OperationKind::Constant {
                        dest: ValueId(0),
                        value: Constant::String("graph child".to_owned()),
                    },
                },
                Operation {
                    span: Span::default(),
                    kind: OperationKind::ValueArray {
                        dest: ValueId(1),
                        values: vec![ValueId(0)],
                    },
                },
                Operation {
                    span: Span::default(),
                    kind: OperationKind::Collect,
                },
                Operation {
                    span: Span::default(),
                    kind: OperationKind::ValueArrayGet {
                        dest: ValueId(2),
                        array: ValueId(1),
                        index: 0,
                    },
                },
                Operation {
                    span: Span::default(),
                    kind: OperationKind::Collect,
                },
                Operation {
                    span: Span::default(),
                    kind: OperationKind::Print {
                        values: vec![ValueId(2)],
                    },
                },
            ],
            terminator: Terminator::Return { code: 0 },
        }],
    };
    let program = Program {
        filename: "<native-test>".to_owned(),
        line_starts: vec![0],
        entry: FunctionId(0),
        functions: vec![function],
    };
    let artifact = rimera_compiler::build_mir(&request, &program).unwrap();
    let result = run(&artifact.executable);
    assert_eq!(result.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&result.stdout), "graph child\n");
    assert!(result.stderr.is_empty());
}

#[test]
fn dead_values_are_reclaimed_under_a_small_heap_limit() {
    let output = output("gc_reclaims_dead");
    let mut request = request("gc_reclaims_dead.py", output);
    request.heap_limit_bytes = Some(12 * 1024);
    let artifact = rimera_compiler::build(request).unwrap();
    let result = run(&artifact.executable);
    assert_eq!(result.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&result.stdout),
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\n"
    );
    assert!(result.stderr.is_empty());
}

#[test]
fn reachable_values_over_heap_limit_fail_deterministically() {
    let output = output("gc_limit_reachable");
    let mut request = request("gc_limit_reachable.py", output);
    request.heap_limit_bytes = Some(12 * 1024);
    let artifact = rimera_compiler::build(request).unwrap();
    let result = run(&artifact.executable);
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.ends_with("MemoryError: managed heap limit exceeded\n"),
        "{stderr}"
    );
}

#[test]
fn source_pipeline_compiles_core_python_subset() {
    let output = output("core");
    let artifact = rimera_compiler::build(request("core.py", output)).unwrap();
    let result = run(&artifact.executable);
    assert_eq!(
        result.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&result.stdout),
        "0\n1\n2\n123456789012345678901234567890\n-3 2\nnative rimera\n"
    );
}

#[test]
fn functions_recursion_full_binding_and_closures_match_cpython() {
    let output = output("functions");
    let request = request("functions.py", output);
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(
        native.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn call_binding_failures_match_cpython_312_messages() {
    let output = output("call_errors");
    let request = request("call_errors.py", output);
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(
        native.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn lambdas_use_the_same_closure_and_full_signature_abi_as_functions() {
    let output = output("lambdas");
    let request = request("lambdas.py", output);
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(
        native.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn global_local_and_free_name_failures_match_cpython_312() {
    let output = output("scopes");
    let request = request("scopes.py", output);
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(
        native.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
}

#[test]
fn arguments_defaults_and_closure_cells_survive_gc_during_deep_recursion() {
    let output = output("deep_calls_gc");
    let request = request("deep_calls_gc.py", output);
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(
        native.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
}

#[test]
fn structured_exception_handlers_else_finally_and_reraise_match_cpython() {
    let output = output("exceptions");
    let request = request("exceptions.py", output);
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(
        native.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
}

#[test]
fn return_break_and_continue_preserve_finally_semantics() {
    let output = output("exception_control_flow");
    let request = request("exception_control_flow.py", output);
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(
        native.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
}

#[test]
fn uncaught_native_exception_records_each_python_frame() {
    let output = output("traceback");
    let request = request("traceback.py", output);
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    assert_eq!(native.status.code(), Some(1));
    assert!(native.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&native.stderr);
    assert!(
        stderr.starts_with("Traceback (most recent call last):\n"),
        "{stderr}"
    );
    let module = stderr.find("in <module>").expect("module frame");
    let outer = stderr.find("in outer").expect("outer frame");
    let inner = stderr.find("in inner").expect("inner frame");
    assert!(module < outer && outer < inner, "{stderr}");
    assert!(
        stderr.ends_with("ZeroDivisionError: integer division or modulo by zero\n"),
        "{stderr}"
    );
}

#[test]
fn explicit_exception_cause_and_context_suppression_are_rendered() {
    let caused_output = output("raise_from");
    let caused = rimera_compiler::build(request("raise_from.py", caused_output)).unwrap();
    let caused = run(&caused.executable);
    assert_eq!(caused.status.code(), Some(1));
    let caused_stderr = String::from_utf8_lossy(&caused.stderr);
    assert!(
        caused_stderr.contains("ValueError: cause"),
        "{caused_stderr}"
    );
    assert!(
        caused_stderr.contains("The above exception was the direct cause"),
        "{caused_stderr}"
    );
    assert!(
        caused_stderr.ends_with("RuntimeError: wrapped\n"),
        "{caused_stderr}"
    );

    let suppressed_output = output("raise_from_none");
    let suppressed =
        rimera_compiler::build(request("raise_from_none.py", suppressed_output)).unwrap();
    let suppressed = run(&suppressed.executable);
    assert_eq!(suppressed.status.code(), Some(1));
    let suppressed_stderr = String::from_utf8_lossy(&suppressed.stderr);
    assert!(
        !suppressed_stderr.contains("ZeroDivisionError"),
        "{suppressed_stderr}"
    );
    assert!(
        !suppressed_stderr.contains("During handling"),
        "{suppressed_stderr}"
    );
    assert!(
        suppressed_stderr.ends_with("RuntimeError: wrapped\n"),
        "{suppressed_stderr}"
    );
}

#[test]
fn exception_raised_by_finally_keeps_the_pending_exception_as_context() {
    let artifact =
        rimera_compiler::build(request("finally_context.py", output("finally_context"))).unwrap();
    let result = run(&artifact.executable);
    assert_eq!(result.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("ZeroDivisionError: integer division or modulo by zero"),
        "{stderr}"
    );
    assert!(
        stderr.contains("During handling of the above exception"),
        "{stderr}"
    );
    assert!(
        stderr.ends_with("RuntimeError: finally failed\n"),
        "{stderr}"
    );
}

#[test]
fn exception_group_handlers_split_nested_children_in_source_order() {
    let output = output("exception_groups");
    let request = request("exception_groups.py", output);
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(
        native.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
}

#[test]
fn exception_group_unmatched_and_handler_failures_remain_visible() {
    let unmatched = rimera_compiler::build(request(
        "exception_group_unmatched.py",
        output("exception_group_unmatched"),
    ))
    .unwrap();
    let unmatched = run(&unmatched.executable);
    assert_eq!(unmatched.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&unmatched.stdout),
        "handled value\n"
    );
    let stderr = String::from_utf8_lossy(&unmatched.stderr);
    assert!(stderr.contains("ExceptionGroup: group"), "{stderr}");
    assert!(stderr.contains("TypeError: type"), "{stderr}");
    assert!(!stderr.contains("ValueError: value"), "{stderr}");

    let handler_error = rimera_compiler::build(request(
        "exception_group_handler_error.py",
        output("exception_group_handler_error"),
    ))
    .unwrap();
    let handler_error = run(&handler_error.executable);
    assert_eq!(handler_error.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&handler_error.stdout),
        "type handler still ran\n"
    );
    let stderr = String::from_utf8_lossy(&handler_error.stderr);
    assert!(stderr.contains("RuntimeError: handler failed"), "{stderr}");
    assert!(!stderr.contains("TypeError: type"), "{stderr}");

    let reraised = rimera_compiler::build(request(
        "exception_group_reraise.py",
        output("exception_group_reraise"),
    ))
    .unwrap();
    let reraised = run(&reraised.executable);
    assert_eq!(reraised.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&reraised.stderr);
    assert!(stderr.contains("ExceptionGroup: original"), "{stderr}");
    assert!(stderr.contains("ValueError: value"), "{stderr}");
    assert!(stderr.contains("TypeError: type"), "{stderr}");
}

#[test]
fn except_star_rejects_control_flow_that_would_escape_its_handler() {
    let output = output("invalid_except_star_control");
    let diagnostics =
        rimera_compiler::build(request("invalid_except_star_control.py", output.clone()))
            .unwrap_err();
    assert!(diagnostics.to_string().contains("RIM-SEMA-001"));
    assert!(
        diagnostics
            .to_string()
            .contains("not allowed in an `except*` handler")
    );
    assert!(!output.exists());
}

#[test]
fn false_branch_executes_else_suite() {
    let output = output("false_branch");
    let artifact = rimera_compiler::build(request("false_branch.py", output)).unwrap();
    let result = run(&artifact.executable);
    assert_eq!(result.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&result.stdout), "false branch\n");
}

#[test]
fn literals_and_operators_match_cpython() {
    let output = output("operators");
    let request = request("operators.py", output);
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), Some(0));
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
}

#[test]
fn while_with_false_initial_condition_skips_its_body() {
    let output = output("while_skipped");
    let artifact = rimera_compiler::build(request("while_skipped.py", output)).unwrap();
    let result = run(&artifact.executable);
    assert_eq!(result.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&result.stdout), "done\n");
    assert!(result.stderr.is_empty());
}

#[test]
fn python_source_survives_automatic_gc_pressure() {
    let output = output("gc_pressure");
    let artifact = rimera_compiler::build(request("gc_pressure.py", output)).unwrap();
    let result = run(&artifact.executable);
    assert_eq!(result.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&result.stdout),
        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\n"
    );
    assert!(result.stderr.is_empty());
}

#[test]
fn intermediate_object_is_kept_under_the_project_rimera_cache() {
    let output = output("cached_object");
    let mut request = request("hello.py", output);
    request.project_root =
        std::env::temp_dir().join(format!("rimera-cache-project-{}", std::process::id()));
    relocate_entry_to_project(&mut request);
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let expected_cache = request.project_root.join(".rimera");
    assert_eq!(artifact.cache_dir, expected_cache);
    assert!(artifact.object.starts_with(expected_cache.join("objects")));
    assert_eq!(
        artifact
            .object
            .extension()
            .and_then(|extension| extension.to_str()),
        Some("o")
    );
    assert!(artifact.object.is_file());
}

#[test]
fn debug_build_writes_a_readable_python_ir_dump() {
    let output = output("ir_dump");
    let mut request = request("hello.py", output);
    request.project_root =
        std::env::temp_dir().join(format!("rimera-ir-project-{}", std::process::id()));
    relocate_entry_to_project(&mut request);
    request.debug = true;
    let artifact = rimera_compiler::build(request).unwrap();
    let dump = artifact.ir_dump.expect("debug builds produce an IR dump");
    assert_eq!(
        dump.extension().and_then(|extension| extension.to_str()),
        Some("py")
    );
    assert!(dump.to_string_lossy().ends_with(".ir.py"));
    let contents = fs::read_to_string(dump).unwrap();
    assert!(contents.starts_with("# IR REPRESENTATION\n"));
    assert!(contents.contains("print_literal(\"hello\")"));
    assert!(!contents.contains("global_get(\"print\")"));
    assert!(!contents.contains(" = call(v"));
}

#[test]
fn compiler_rejects_debug_ir_dumps_as_source_entries() {
    let output = output("ir_source_rejection");
    let mut debug_request = request("hello.py", output.clone());
    debug_request.project_root = std::env::temp_dir().join(format!(
        "rimera-ir-rejection-project-{}",
        std::process::id()
    ));
    relocate_entry_to_project(&mut debug_request);
    debug_request.debug = true;
    let artifact = rimera_compiler::build(debug_request).unwrap();
    let mut ir_request = request("hello.py", output);
    ir_request.entry = artifact.ir_dump.unwrap();
    let diagnostics = rimera_compiler::build(ir_request).unwrap_err();
    assert!(diagnostics.to_string().contains("RIM-INPUT-002"));
    assert!(
        diagnostics
            .to_string()
            .contains("read-only debugging artifacts")
    );
}

#[test]
fn runtime_failure_is_nonzero_and_structured() {
    let output = output("runtime_error");
    let request = request("runtime_error.py", output);
    let entry = request.entry.clone();
    let artifact = rimera_compiler::build(request).unwrap();
    let result = run(&artifact.executable);
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&result.stderr),
        format!(
            "Traceback (most recent call last):\n  File {:?}, line 1, in <module>\nZeroDivisionError: integer division or modulo by zero\n",
            entry
        )
    );
}

#[test]
fn final_artifact_has_no_legacy_python_or_unwind_symbols() {
    let output = output("symbols");
    let artifact = rimera_compiler::build(request("hello.py", output)).unwrap();
    let symbols = Command::new("nm")
        .arg(&artifact.executable)
        .output()
        .unwrap();
    assert!(symbols.status.success());
    let symbols = String::from_utf8_lossy(&symbols.stdout);
    for line in symbols.lines() {
        let symbol = line
            .split_whitespace()
            .last()
            .unwrap_or("")
            .trim_start_matches('_');
        let forbidden = symbol.starts_with("rv_")
            || symbol.starts_with("Py_")
            || symbol.contains("PyObject")
            || symbol == "setjmp"
            || symbol == "longjmp";
        assert!(!forbidden, "artifact contains forbidden symbol `{symbol}`");
    }
}

#[test]
fn unsupported_and_malformed_source_emit_no_artifact() {
    for (fixture, code) in [
        ("unsupported.py", "RIM-CAP-G7-03"),
        ("malformed.py", "RIM-PARSE-001"),
    ] {
        let output = output(fixture);
        let diagnostics = rimera_compiler::build(request(fixture, output.clone())).unwrap_err();
        assert!(diagnostics.to_string().contains(code));
        assert!(!output.exists());
    }
}

#[test]
fn gate4_invalid_target_forms_have_stable_semantic_diagnostics() {
    let fixture = "gate4_cap_multiple_star.py";
    let code = "RIM-SEMA-001";
    let output = output(fixture);
    let diagnostics = rimera_compiler::build(request(fixture, output.clone())).unwrap_err();
    let diagnostic = &diagnostics.as_slice()[0];
    assert_eq!(diagnostic.code, code, "fixture: {fixture}");
    assert!(
        diagnostic.span.end > diagnostic.span.start,
        "fixture: {fixture}: {diagnostic:?}"
    );
    assert!(!output.exists(), "fixture emitted an artifact: {fixture}");
}

#[test]
fn gate7_slice7_lazy_type_parameter_bounds_have_a_stable_no_artifact_boundary() {
    let fixture = "gate7_cap_type_param_bound.py";
    let output = output(fixture);
    let diagnostics = rimera_compiler::build(request(fixture, output.clone())).unwrap_err();
    let diagnostic = &diagnostics.as_slice()[0];
    assert_eq!(diagnostic.code, "RIM-CAP-G7-03");
    assert!(diagnostic.message.contains("lazy PEP 695"));
    assert!(diagnostic.span.end > diagnostic.span.start);
    assert!(!output.exists());
}

#[test]
fn gate5_slice1_existing_function_scope_exception_kernel_remains_native_and_differential() {
    for fixture in [
        "functions.py",
        "lambdas.py",
        "scopes.py",
        "exceptions.py",
        "exception_control_flow.py",
        "gate4_composition.py",
    ] {
        assert_gate4_fixture_matches_cpython(fixture);
    }
}

#[test]
fn gate5_slice1_later_gate_boundaries_are_stable_and_emit_no_artifact() {
    for (fixture, code) in [
        ("gate5_cap_import.py", "RIM-IMPORT-001"),
        ("gate7_cap_type_param_bound.py", "RIM-CAP-G7-03"),
    ] {
        let output = output(fixture);
        let diagnostics = rimera_compiler::build(request(fixture, output.clone())).unwrap_err();
        let diagnostic = &diagnostics.as_slice()[0];
        assert_eq!(diagnostic.code, code, "fixture: {fixture}");
        assert!(
            diagnostic.span.end > diagnostic.span.start,
            "fixture: {fixture}"
        );
        assert!(!output.exists(), "fixture emitted an artifact: {fixture}");
    }
}

#[test]
fn gate5_slice2_function_metadata_defaults_and_decorators_match_cpython_312() {
    assert_gate4_fixture_matches_cpython("gate5_function_metadata.py");
}

#[test]
fn gate5_slice2_function_metadata_graphs_survive_gc_and_dead_cycles_collect() {
    assert_gate4_fixture_matches_cpython_with_heap_limit(
        "gate5_function_metadata_gc.py",
        Some(32_768),
    );
}

#[test]
fn gate5_slice3_authoritative_binding_and_activation_isolation_match_cpython_312() {
    assert_gate4_fixture_matches_cpython_with_heap_limit(
        "gate5_binding_activation.py",
        Some(65_536),
    );
}

#[test]
fn gate5_slice4_scope_matrix_matches_cpython_312_under_gc_pressure() {
    assert_gate4_fixture_matches_cpython_with_heap_limit("gate5_scope_matrix.py", Some(96_000));
}

#[test]
fn gate5_slice4_declaration_conflicts_have_stable_spans_and_emit_no_artifact() {
    for fixture in [
        "gate5_scope_conflict_global_nonlocal.py",
        "gate5_scope_conflict_parameter_global.py",
        "gate5_scope_conflict_missing_nonlocal.py",
    ] {
        let output = output(fixture);
        let diagnostics = rimera_compiler::build(request(fixture, output.clone())).unwrap_err();
        let diagnostic = &diagnostics.as_slice()[0];
        assert_eq!(diagnostic.code, "RIM-SEMA-001", "fixture: {fixture}");
        assert!(
            diagnostic.span.end > diagnostic.span.start,
            "declaration diagnostic has no source span: {fixture}"
        );
        assert!(!output.exists(), "fixture emitted an artifact: {fixture}");
    }
}

#[test]
fn gate5_slice5_exception_objects_tracebacks_and_normalization_match_cpython_312() {
    assert_gate4_fixture_matches_cpython_with_heap_limit(
        "gate5_exception_objects.py",
        Some(96_000),
    );
}

#[test]
fn gate5_slice6_handler_capture_survives_generator_suspension_and_cleanup() {
    assert_gate4_fixture_matches_cpython_with_heap_limit(
        "gate5_handler_capture_suspend.py",
        Some(96_000),
    );
}

#[test]
fn gate5_slice6_propagation_chaining_groups_and_cleanup_match_cpython_312() {
    for fixture in [
        "exceptions.py",
        "exception_control_flow.py",
        "exception_groups.py",
        "gate6_throw_close.py",
        "gate6_cleanup_suspend.py",
    ] {
        assert_gate4_fixture_matches_cpython_with_heap_limit(fixture, Some(128_000));
    }
}

#[test]
fn gate5_slice7_cross_feature_composition_matches_cpython_312_under_gc_pressure() {
    for fixture in [
        "gate4_composition.py",
        "gate6_composition.py",
        "gate5_scope_matrix.py",
        "gate5_function_metadata_gc.py",
    ] {
        assert_gate4_fixture_matches_cpython_with_heap_limit(fixture, Some(160_000));
    }
}

#[test]
fn gate7_slice1_deferred_reflection_boundaries_are_stable_and_emit_no_artifact() {
    for (fixture, code) in [
        ("gate7_cap_eval.py", "RIM-CAP-G7-02"),
        ("gate7_cap_exec.py", "RIM-CAP-G7-02"),
        ("gate7_cap_compile.py", "RIM-CAP-G7-02"),
        ("gate7_cap_unregistered_import.py", "RIM-IMPORT-001"),
        ("gate7_cap_dotted_import.py", "RIM-IMPORT-001"),
        ("gate7_cap_type_param_bound.py", "RIM-CAP-G7-03"),
    ] {
        let output = output(fixture);
        let diagnostics = rimera_compiler::build(request(fixture, output.clone())).unwrap_err();
        let diagnostic = &diagnostics.as_slice()[0];
        assert_eq!(diagnostic.code, code, "fixture: {fixture}");
        assert!(
            diagnostic.span.end > diagnostic.span.start,
            "fixture: {fixture}"
        );
        assert!(!output.exists(), "fixture emitted an artifact: {fixture}");
    }
}

#[test]
fn gate7_slice1_pulled_forward_import_foundation_matches_cpython_312() {
    assert_gate4_fixture_matches_cpython("gate7_import_foundation.py");
}

#[test]
fn gate7_slice1_dynamic_builtin_names_allow_user_rebinding() {
    assert_gate4_fixture_matches_cpython("gate7_dynamic_builtin_rebinding.py");
}

#[test]
fn gate7_slice2_namespace_views_match_cpython_312() {
    assert_gate4_fixture_matches_cpython("gate7_namespace_views.py");
}

#[test]
fn gate7_slice2_namespace_views_survive_gc_and_dead_activation_cycles_collect() {
    assert_gate4_fixture_matches_cpython_with_heap_limit(
        "gate7_namespace_views_gc.py",
        Some(96_000),
    );
}

#[test]
fn gate7_slice3_identity_type_relations_and_attribute_reflection_match_cpython_312() {
    assert_gate4_fixture_matches_cpython_with_heap_limit(
        "gate7_identity_attribute_reflection.py",
        Some(128_000),
    );
}

#[test]
fn gate7_slice4_function_closure_signature_and_code_metadata_match_cpython_312() {
    assert_gate4_fixture_matches_cpython("gate7_function_code_metadata.py");
}

#[test]
fn gate7_slice4_retained_code_and_closure_metadata_survive_gc_pressure() {
    assert_gate4_fixture_matches_cpython_with_heap_limit(
        "gate7_function_code_metadata_gc.py",
        Some(96_000),
    );
}

#[test]
fn gate7_slice5_exception_traceback_and_frame_metadata_match_cpython_312() {
    assert_gate4_fixture_matches_cpython("gate7_exception_traceback_frames.py");
}

#[test]
fn gate7_slice5_retained_traceback_frames_survive_gc_pressure() {
    assert_gate4_fixture_matches_cpython_with_heap_limit(
        "gate7_exception_traceback_frames_gc.py",
        Some(128_000),
    );
}

#[test]
fn gate7_slice6_generator_identity_state_and_suspension_metadata_match_cpython_312() {
    assert_gate4_fixture_matches_cpython_with_heap_limit(
        "gate7_generator_metadata.py",
        Some(160_000),
    );
}

#[test]
fn gate7_slice6_retained_generator_frames_survive_gc_and_terminal_detach() {
    assert_gate4_fixture_matches_cpython_with_heap_limit(
        "gate7_generator_metadata_gc.py",
        Some(128_000),
    );
}

#[test]
fn gate7_slice7_python312_type_parameter_metadata_matches_cpython() {
    assert_gate4_fixture_matches_cpython("gate7_type_parameters.py");
}

#[test]
fn gate7_slice7_type_method_tables_and_class_metadata_match_cpython() {
    assert_gate4_fixture_matches_cpython("gate7_type_metadata.py");
}

#[test]
fn gate7_slice7_type_parameter_graphs_survive_forced_gc() {
    assert_gate4_fixture_matches_cpython_with_heap_limit(
        "gate7_type_parameters_gc.py",
        Some(48_000),
    );
}

#[test]
fn gate7_slice8_python_level_buffer_protocol_matches_cpython_312() {
    assert_gate4_fixture_matches_cpython_with_heap_limit("gate7_pep688_buffer.py", Some(96_000));
}

#[test]
fn gate7_slice8_provider_graph_survives_forced_gc_and_heap_pressure() {
    assert_gate4_fixture_matches_cpython_with_heap_limit("gate7_pep688_buffer_gc.py", Some(48_000));
}

#[test]
fn gate7_slice8_release_callback_exception_semantics_match_cpython_312() {
    let fixture = "gate7_pep688_release_callback_error.py";
    let request = request(fixture, output(fixture));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code(), "{fixture}");
    assert_eq!(native.stdout, python.stdout, "{fixture}");
    // CPython reports __release_buffer__ callback failures through the
    // unraisable hook, whose stderr includes a nondeterministic object address.
    // The semantic differential is release() success + callback side effects;
    // Rimera's runtime unit proof separately verifies the export is dropped.
    assert!(
        native.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert!(String::from_utf8_lossy(&python.stderr).contains("Exception ignored in:"));
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate7_slice9_reflection_mutation_composition_matches_cpython_312_under_gc_pressure() {
    assert_gate4_fixture_matches_cpython_with_heap_limit(
        "gate7_reflection_mutation_composition.py",
        Some(192_000),
    );
}

#[test]
fn gate8_async_with_boundary_is_owned_by_gate10_native_lowering() {
    let fixture = "gate8_async_with_deferred.py";
    let mut build_request = request(fixture, output(fixture));
    build_request.debug = true;
    let artifact = rimera_compiler::build(build_request).unwrap();
    let dump = fs::read_to_string(artifact.ir_dump.as_ref().expect("debug MIR dump")).unwrap();
    assert!(dump.contains("await_iter("), "{dump}");
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate8_slice2_single_manager_lookup_enter_body_and_exit_match_cpython_312() {
    assert_gate4_fixture_matches_cpython_with_heap_limit("gate8_single_manager.py", Some(160_000));
}

#[test]
fn gate8_slice2_single_manager_lookup_and_call_failures_match_cpython_312() {
    assert_gate4_fixture_matches_cpython_with_heap_limit(
        "gate8_single_manager_failures.py",
        Some(160_000),
    );
}

#[test]
fn gate8_slice2_dead_manager_cycles_collect_under_heap_pressure() {
    assert_gate4_fixture_matches_cpython_with_heap_limit(
        "gate8_single_manager_gc.py",
        Some(96_000),
    );
}

#[test]
fn gate8_slice3_targets_multiple_managers_and_partial_entry_match_cpython_312() {
    assert_gate4_fixture_matches_cpython_with_heap_limit(
        "gate8_targets_multiple.py",
        Some(192_000),
    );
}

#[test]
fn gate8_slice4_control_transfers_nested_cleanup_and_class_suites_match_cpython_312() {
    assert_gate4_fixture_matches_cpython_with_heap_limit("gate8_control_cleanup.py", Some(192_000));
}

#[test]
fn gate8_slice5_exception_triples_suppression_replacement_and_chaining_match_cpython_312() {
    assert_gate4_fixture_matches_cpython_with_heap_limit(
        "gate8_exceptional_cleanup.py",
        Some(224_000),
    );
}

#[test]
fn gate8_slice6_suspension_throw_close_and_delegated_cleanup_match_cpython_312() {
    assert_gate4_fixture_matches_cpython_with_heap_limit(
        "gate8_generator_cleanup.py",
        Some(224_000),
    );
}

#[test]
fn gate8_slice7_composition_and_gc_match_cpython_312_under_heap_pressure() {
    assert_gate4_fixture_matches_cpython_with_heap_limit("gate8_composition_gc.py", Some(128_000));
}

#[test]
fn gate6_source_generator_protocol_matches_cpython_312_under_gc_pressure() {
    assert_gate4_fixture_matches_cpython_with_heap_limit(
        "gate6_source_generators.py",
        Some(96_000),
    );
}

#[test]
fn gate6_throw_close_and_pep479_match_cpython_312_under_gc_pressure() {
    assert_gate4_fixture_matches_cpython_with_heap_limit("gate6_throw_close.py", Some(96_000));
}

#[test]
fn gate6_cleanup_suspension_matches_cpython_312_under_gc_pressure() {
    assert_gate4_fixture_matches_cpython_with_heap_limit("gate6_cleanup_suspend.py", Some(96_000));
}

#[test]
fn gate6_yield_from_native_builtin_and_nested_match_cpython_312_under_gc_pressure() {
    assert_gate4_fixture_matches_cpython_with_heap_limit("gate6_yield_from.py", Some(96_000));
}

#[test]
fn gate6_yield_from_user_delegate_matches_cpython_312_under_gc_pressure() {
    assert_gate4_fixture_matches_cpython_with_heap_limit("gate6_yield_from_user.py", Some(96_000));
}

#[test]
fn gate6_composition_matches_cpython_312_under_gc_pressure() {
    assert_gate4_fixture_matches_cpython_with_heap_limit("gate6_composition.py", Some(128_000));
}

fn assert_gate4_fixture_matches_cpython(fixture: &str) {
    assert_gate4_fixture_matches_cpython_with_heap_limit(fixture, None);
}

fn assert_gate4_fixture_matches_cpython_with_heap_limit(
    fixture: &str,
    heap_limit_bytes: Option<u64>,
) {
    let mut request = request(fixture, output(fixture));
    request.heap_limit_bytes = heap_limit_bytes;
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(
        native.status.code(),
        python.status.code(),
        "{fixture}\nnative stdout:\n{}\nnative stderr:\n{}\npython stderr:\n{}",
        String::from_utf8_lossy(&native.stdout),
        String::from_utf8_lossy(&native.stderr),
        String::from_utf8_lossy(&python.stderr),
    );
    assert_eq!(native.stdout, python.stdout, "{fixture}");
    assert_eq!(native.stderr, python.stderr, "{fixture}");
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate4_assignment_expressions_match_cpython_312() {
    assert_gate4_fixture_matches_cpython("gate4_assignment_expressions.py");
}

#[test]
fn gate4_annotations_match_cpython_312() {
    for fixture in [
        "gate4_annotations.py",
        "gate4_cap_annotation.py",
        "gate4_type_comments.py",
    ] {
        assert_gate4_fixture_matches_cpython(fixture);
    }
}

#[test]
fn gate4_fstrings_match_cpython_312() {
    assert_gate4_fixture_matches_cpython("gate4_fstrings.py");
}

#[test]
fn gate4_comprehension_scope_matches_cpython_312() {
    assert_gate4_fixture_matches_cpython("gate4_comprehension_scope.py");
}

#[test]
fn gate4_generator_expressions_match_cpython_312() {
    assert_gate4_fixture_matches_cpython("gate4_generator_expressions.py");
    assert_gate4_fixture_matches_cpython("gate4_cap_generator_expression.py");
}

#[test]
fn gate4_generator_expression_state_survives_forced_gc() {
    assert_gate4_fixture_matches_cpython_with_heap_limit("gate4_generator_gc.py", Some(32_768));
}

#[test]
fn gate4_match_core_matches_cpython_312() {
    assert_gate4_fixture_matches_cpython("gate4_match_core.py");
    assert_gate4_fixture_matches_cpython("gate4_cap_match.py");
}

#[test]
fn gate4_match_tentative_bindings_survive_forced_gc() {
    assert_gate4_fixture_matches_cpython_with_heap_limit("gate4_match_gc.py", Some(32_768));
}

#[test]
fn gate4_sequence_and_mapping_patterns_match_cpython_312() {
    assert_gate4_fixture_matches_cpython("gate4_cap_match_sequence.py");
    assert_gate4_fixture_matches_cpython("gate4_match_sequence_mapping.py");
}

#[test]
fn gate4_sequence_and_mapping_patterns_survive_forced_gc() {
    assert_gate4_fixture_matches_cpython_with_heap_limit(
        "gate4_match_sequence_mapping_gc.py",
        Some(32_768),
    );
}

#[test]
fn gate4_class_patterns_match_cpython_312() {
    assert_gate4_fixture_matches_cpython("gate4_cap_match_class.py");
    assert_gate4_fixture_matches_cpython("gate4_match_class_patterns.py");
}

#[test]
fn gate4_class_patterns_survive_forced_gc() {
    assert_gate4_fixture_matches_cpython_with_heap_limit("gate4_match_class_gc.py", Some(32_768));
}

#[test]
fn gate4_cross_feature_composition_matches_cpython_312() {
    for fixture in ["gate4_composition.py", "gate4_composition_failures.py"] {
        assert_gate4_fixture_matches_cpython(fixture);
    }
}

#[test]
fn gate4_cross_feature_traceback_shape_matches_cpython_312() {
    let fixture = "gate4_composition_traceback.py";
    let request = request(fixture, output(fixture));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    let native_stderr = String::from_utf8_lossy(&native.stderr);
    let python_stderr = String::from_utf8_lossy(&python.stderr);
    for frame in [
        "line 10, in <module>",
        "line 7, in render",
        "line 3, in __format__",
    ] {
        assert!(
            native_stderr.contains(frame),
            "native traceback missed {frame}: {native_stderr}"
        );
        assert!(
            python_stderr.contains(frame),
            "CPython traceback missed {frame}: {python_stderr}"
        );
    }
    assert!(!native_stderr.contains("in <listcomp>"), "{native_stderr}");
    assert!(!python_stderr.contains("in <listcomp>"), "{python_stderr}");
    assert!(native_stderr.ends_with("ValueError: composition traceback\n"));
    assert!(python_stderr.ends_with("ValueError: composition traceback\n"));
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate4_cross_feature_composition_survives_forced_gc() {
    assert_gate4_fixture_matches_cpython_with_heap_limit("gate4_composition_gc.py", Some(32_768));
}

#[test]
fn gate4_heap_limit_failure_preserves_cross_feature_state() {
    let fixture = "gate4_composition_heap_limit.py";
    let mut request = request(fixture, output(fixture));
    request.heap_limit_bytes = Some(90_000);
    let artifact = rimera_compiler::build(request).unwrap();
    let native = run(&artifact.executable);
    assert_eq!(native.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&native.stdout),
        "before {'stable': [1, 2], 'other': 3} 10 second-old third-old pattern-old\nmemory {'stable': [1, 2], 'other': 3} 10 second-old third-old pattern-old\nafter 2 2\n"
    );
    assert!(native.stderr.is_empty());
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate4_match_semantic_failures_are_stable_and_emit_no_artifact() {
    for (fixture, expected_fragment) in [
        ("gate4_match_duplicate_capture.py", "more than once"),
        ("gate4_match_inconsistent_or.py", "must bind the same names"),
        (
            "gate4_match_unreachable_or.py",
            "makes later alternatives unreachable",
        ),
    ] {
        let output = output(fixture);
        let diagnostics = rimera_compiler::build(request(fixture, output.clone())).unwrap_err();
        let diagnostic = &diagnostics.as_slice()[0];
        assert_eq!(diagnostic.code, "RIM-SEMA-001", "fixture: {fixture}");
        assert!(
            diagnostic.span.end > diagnostic.span.start,
            "{diagnostic:?}"
        );
        assert!(
            diagnostic.message.contains(expected_fragment),
            "fixture: {fixture}: {diagnostic:?}"
        );
        assert!(!output.exists(), "fixture emitted an artifact: {fixture}");
    }
}

#[test]
fn gate4_list_comprehensions_match_cpython_312() {
    for fixture in ["gate4_cap_list_comp.py", "gate4_list_comprehensions.py"] {
        assert_gate4_fixture_matches_cpython(fixture);
    }
}

#[test]
fn gate4_list_comprehension_growth_obeys_managed_heap_limits() {
    let fixture = "gate4_list_comprehension_growth.py";
    let normal_request = request(fixture, output("gate4_list_comp_growth_ok"));
    let normal_artifact = rimera_compiler::build(normal_request).unwrap();
    let normal = run(&normal_artifact.executable);
    assert_eq!(normal.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&normal.stdout), "4096\n");
    assert!(normal.stderr.is_empty());
    assert_native_only_artifact(&normal_artifact.executable);

    let mut limited_request = request(fixture, output("gate4_list_comp_growth_limit"));
    limited_request.heap_limit_bytes = Some(70_000);
    let limited_artifact = rimera_compiler::build(limited_request).unwrap();
    let limited = run(&limited_artifact.executable);
    assert_eq!(limited.status.code(), Some(1));
    assert!(limited.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&limited.stderr);
    assert!(
        stderr.ends_with("MemoryError: managed heap limit exceeded\n"),
        "{stderr}"
    );
    assert_native_only_artifact(&limited_artifact.executable);
}

#[test]
fn gate4_set_and_dictionary_comprehensions_match_cpython_312() {
    assert_gate4_fixture_matches_cpython("gate4_set_dict_comprehensions.py");
}

#[test]
fn gate4_dictionary_unpacking_matches_cpython_312() {
    for fixture in ["gate4_dictionary_unpacking.py", "gate4_cap_dict_unpack.py"] {
        let request = request(fixture, output(fixture));
        let artifact = rimera_compiler::build(request.clone()).unwrap();
        let native = run(&artifact.executable);
        let python = Command::new("/opt/homebrew/bin/python3.12")
            .arg(&request.entry)
            .output()
            .unwrap();
        assert_eq!(native.status.code(), python.status.code(), "{fixture}");
        assert_eq!(native.stdout, python.stdout, "{fixture}");
        assert_eq!(native.stderr, python.stderr, "{fixture}");
        assert_native_only_artifact(&artifact.executable);
    }
}

#[test]
fn gate4_expanded_calls_match_cpython_312() {
    for fixture in ["gate4_expanded_calls.py", "gate4_cap_expanded_call.py"] {
        let request = request(fixture, output(fixture));
        let artifact = rimera_compiler::build(request.clone()).unwrap();
        let native = run(&artifact.executable);
        let python = Command::new("/opt/homebrew/bin/python3.12")
            .arg(&request.entry)
            .output()
            .unwrap();
        assert_eq!(native.status.code(), python.status.code(), "{fixture}");
        assert_eq!(native.stdout, python.stdout, "{fixture}");
        assert_eq!(native.stderr, python.stderr, "{fixture}");
        assert_native_only_artifact(&artifact.executable);
    }
}

#[test]
fn gate4_for_destructuring_matches_cpython_312() {
    for fixture in ["gate4_for_unpacking.py", "gate4_cap_for_unpack.py"] {
        let request = request(fixture, output(fixture));
        let artifact = rimera_compiler::build(request.clone()).unwrap();
        let native = run(&artifact.executable);
        let python = Command::new("/opt/homebrew/bin/python3.12")
            .arg(&request.entry)
            .output()
            .unwrap();
        assert_eq!(native.status.code(), python.status.code(), "{fixture}");
        assert_eq!(native.stdout, python.stdout, "{fixture}");
        assert_eq!(native.stderr, python.stderr, "{fixture}");
        assert_native_only_artifact(&artifact.executable);
    }
}

#[test]
fn gate4_boolean_short_circuit_finishing_matches_cpython_312() {
    for fixture in [
        "boolean_short_circuit.py",
        "gate4_boolean_short_circuit_finish.py",
    ] {
        let request = request(fixture, output(fixture));
        let artifact = rimera_compiler::build(request.clone()).unwrap();
        let native = run(&artifact.executable);
        let python = Command::new("/opt/homebrew/bin/python3.12")
            .arg(&request.entry)
            .output()
            .unwrap();
        assert_eq!(native.status.code(), python.status.code(), "{fixture}");
        assert_eq!(native.stdout, python.stdout, "{fixture}");
        assert_eq!(native.stderr, python.stderr, "{fixture}");
        assert_native_only_artifact(&artifact.executable);
    }
}

#[test]
fn gate4_comparison_chains_match_cpython_312() {
    for fixture in ["gate4_comparison_chains.py", "gate4_cap_compare_chain.py"] {
        let request = request(fixture, output(fixture));
        let artifact = rimera_compiler::build(request.clone()).unwrap();
        let native = run(&artifact.executable);
        let python = Command::new("/opt/homebrew/bin/python3.12")
            .arg(&request.entry)
            .output()
            .unwrap();
        assert_eq!(native.status.code(), python.status.code(), "{fixture}");
        assert_eq!(native.stdout, python.stdout, "{fixture}");
        assert_eq!(native.stderr, python.stderr, "{fixture}");
        assert_native_only_artifact(&artifact.executable);
    }
}

#[test]
fn gate4_slicing_and_extended_subscripts_match_cpython_312() {
    let fixture = "gate4_slicing_extended.py";
    let request = request(fixture, output(fixture));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate4_augmented_deletion_and_assertions_match_cpython_312() {
    for fixture in ["gate4_statements_slice11.py", "gate4_cap_assert.py"] {
        let request = request(fixture, output(fixture));
        let artifact = rimera_compiler::build(request.clone()).unwrap();
        let native = run(&artifact.executable);
        let python = Command::new("/opt/homebrew/bin/python3.12")
            .arg(&request.entry)
            .output()
            .unwrap();
        assert_eq!(native.status.code(), python.status.code(), "{fixture}");
        assert_eq!(native.stdout, python.stdout, "{fixture}");
        assert_eq!(native.stderr, python.stderr, "{fixture}");
        assert_native_only_artifact(&artifact.executable);
    }
}

#[test]
fn gate4_assert_failure_pins_cpython_type_message_and_frame_positions() {
    let fixture = "gate4_assert_failure.py";
    let request = request(fixture, output(fixture));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    let native_stderr = String::from_utf8_lossy(&native.stderr);
    let python_stderr = String::from_utf8_lossy(&python.stderr);
    for frame in ["line 5, in <module>", "line 2, in fail"] {
        assert!(
            native_stderr.contains(frame),
            "native traceback missed {frame}: {native_stderr}"
        );
        assert!(
            python_stderr.contains(frame),
            "CPython traceback missed {frame}: {python_stderr}"
        );
    }
    assert!(native_stderr.ends_with("AssertionError: boom\n"));
    assert!(python_stderr.ends_with("AssertionError: boom\n"));
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate4_nested_exact_unpacking_matches_cpython_312() {
    let request = request(
        "gate4_nested_unpacking.py",
        output("gate4_nested_unpacking"),
    );
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate4_general_starred_unpacking_matches_cpython_312() {
    let request = request(
        "gate4_starred_unpacking.py",
        output("gate4_starred_unpacking"),
    );
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate4_class_body_recursive_and_chained_unpacking_matches_cpython_312() {
    assert_gate4_fixture_matches_cpython("gate4_class_body_unpacking.py");
}

#[test]
fn gate4_recursive_target_baseline_matches_cpython_312() {
    let baseline_request = request("gate4_target_baseline.py", output("gate4_target_baseline"));
    let artifact = rimera_compiler::build(baseline_request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&baseline_request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);

    for fixture in ["gate4_cap_nested_unpack.py", "gate4_cap_middle_star.py"] {
        let promoted = request(fixture, output(fixture));
        let artifact = rimera_compiler::build(promoted.clone()).unwrap();
        let native = run(&artifact.executable);
        let python = Command::new("/opt/homebrew/bin/python3.12")
            .arg(&promoted.entry)
            .output()
            .unwrap();
        assert_eq!(native.status.code(), python.status.code(), "{fixture}");
        assert_eq!(native.stdout, python.stdout, "{fixture}");
        assert_eq!(native.stderr, python.stderr, "{fixture}");
        assert_native_only_artifact(&artifact.executable);
    }
}

#[test]
fn native_output_matches_cpython_312_for_core_fixture() {
    let output = output("differential");
    let request = request("core.py", output);
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
}

#[test]
fn list_literals_are_native_values_with_python_display_and_truthiness() {
    let output = output("lists");
    let request = request("lists.py", output);
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);

    let symbols = Command::new("nm")
        .arg(&artifact.executable)
        .output()
        .unwrap();
    assert!(symbols.status.success());
    let symbols = String::from_utf8_lossy(&symbols.stdout);
    for symbol in ["Py_", "PyObject", "setjmp", "longjmp"] {
        assert!(
            !symbols.contains(symbol),
            "list artifact contains forbidden symbol `{symbol}`"
        );
    }
}

#[test]
fn sequence_access_matches_cpython_for_negative_indices_and_bounds() {
    let access_output = output("sequence_access");
    let access_request = request("lists.py", access_output);
    let artifact = rimera_compiler::build(access_request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&access_request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);

    let error_output = output("sequence_index_error");
    let error_request = request("sequence_index_error.py", error_output);
    let artifact = rimera_compiler::build(error_request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&error_request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    let stderr = String::from_utf8_lossy(&native.stderr);
    assert!(stderr.starts_with("Traceback (most recent call last):\n"));
    assert!(stderr.contains("sequence_index_error.py\", line 2, in <module>"));
    assert!(stderr.ends_with("IndexError: list index out of range\n"));
}

#[test]
fn list_item_assignment_matches_cpython_and_reports_bounds_errors() {
    let mutation_request = request("list_mutation.py", output("list_mutation"));
    let artifact = rimera_compiler::build(mutation_request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&mutation_request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);

    let error_request = request("list_assignment_error.py", output("list_assignment_error"));
    let artifact = rimera_compiler::build(error_request).unwrap();
    let native = run(&artifact.executable);
    assert_eq!(native.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&native.stderr)
            .ends_with("IndexError: list assignment index out of range\n")
    );
}

#[test]
fn sequence_concatenation_repetition_and_equality_match_cpython() {
    let request = request("sequence_operators.py", output("sequence_operators"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
}

#[test]
fn range_and_for_loops_match_cpython_for_native_iterables() {
    let request = request("for_range.py", output("for_range"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
}

#[test]
fn iter_and_next_use_native_iterator_values_through_generic_calls() {
    let request = request("iter_next.py", output("iter_next"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn dictionaries_and_sets_are_native_gc_traced_collections() {
    let request = request("dict_set.py", output("dict_set"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    let symbols = Command::new("nm")
        .arg(&artifact.executable)
        .output()
        .unwrap();
    let symbols = String::from_utf8_lossy(&symbols.stdout);
    for forbidden in ["Py_", "setjmp", "longjmp", "rimera_compat"] {
        assert!(!symbols.contains(forbidden), "forbidden symbol {forbidden}");
    }
}

#[test]
fn simple_and_final_starred_unpacking_match_cpython() {
    let request = request("unpacking.py", output("unpacking"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
}

#[test]
fn builtin_type_kernel_uses_the_generic_native_call_path() {
    let request = request("type_kernel.py", output("type_kernel"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);

    let symbols = Command::new("nm")
        .arg(&artifact.executable)
        .output()
        .unwrap();
    assert!(symbols.status.success());
    let symbols = String::from_utf8_lossy(&symbols.stdout);
    for forbidden in ["Py_", "PyObject", "setjmp", "longjmp", "rimera_compat"] {
        assert!(!symbols.contains(forbidden), "forbidden symbol {forbidden}");
    }
}

#[test]
fn builtin_type_kernel_reports_cpython_shaped_call_failures() {
    for (fixture, expected) in [
        (
            "type_kernel_invalid_arity.py",
            "TypeError: type() takes 1 or 3 arguments\n",
        ),
        (
            "type_kernel_invalid_class_info.py",
            "TypeError: isinstance() arg 2 must be a type, a tuple of types, or a union\n",
        ),
        (
            "type_kernel_invalid_keyword.py",
            "TypeError: type() takes 1 or 3 arguments\n",
        ),
        (
            "type_kernel_invalid_subclass.py",
            "TypeError: issubclass() arg 1 must be a class\n",
        ),
    ] {
        let request = request(fixture, output(fixture));
        let artifact = rimera_compiler::build(request.clone()).unwrap();
        let native = run(&artifact.executable);
        let python = Command::new("/opt/homebrew/bin/python3.12")
            .arg(&request.entry)
            .output()
            .unwrap();
        assert_eq!(native.status.code(), python.status.code());
        assert_eq!(native.stdout, python.stdout);
        assert!(
            String::from_utf8_lossy(&native.stderr).ends_with(expected),
            "{}",
            String::from_utf8_lossy(&native.stderr)
        );
        assert!(String::from_utf8_lossy(&python.stderr).ends_with(expected));
    }
}

#[test]
fn empty_classes_and_dynamic_type_construction_use_the_native_object_kernel() {
    let request = request("classes.py", output("classes"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);

    let symbols = Command::new("nm")
        .arg(&artifact.executable)
        .output()
        .unwrap();
    assert!(symbols.status.success());
    let symbols = String::from_utf8_lossy(&symbols.stdout);
    for forbidden in ["Py_", "PyObject", "setjmp", "longjmp", "rimera_compat"] {
        assert!(
            !symbols.contains(forbidden),
            "class artifact contains forbidden symbol `{forbidden}`"
        );
    }
}

#[test]
fn class_calls_and_three_argument_type_validation_match_cpython() {
    for (fixture, expected) in [
        (
            "class_call_error.py",
            "TypeError: Empty() takes no arguments\n",
        ),
        (
            "type_class_name_error.py",
            "TypeError: type.__new__() argument 1 must be str, not int\n",
        ),
        (
            "type_class_bases_error.py",
            "TypeError: type.__new__() argument 2 must be tuple, not int\n",
        ),
        (
            "type_class_namespace_error.py",
            "TypeError: type.__new__() argument 3 must be dict, not int\n",
        ),
    ] {
        let request = request(fixture, output(fixture));
        let artifact = rimera_compiler::build(request.clone()).unwrap();
        let native = run(&artifact.executable);
        let python = Command::new("/opt/homebrew/bin/python3.12")
            .arg(&request.entry)
            .output()
            .unwrap();
        assert_eq!(native.status.code(), python.status.code());
        assert_eq!(native.stdout, python.stdout);
        assert!(
            String::from_utf8_lossy(&native.stderr).ends_with(expected),
            "{}",
            String::from_utf8_lossy(&native.stderr)
        );
        assert!(String::from_utf8_lossy(&python.stderr).ends_with(expected));
    }
}

#[test]
fn attributes_and_bound_methods_follow_the_native_object_protocol() {
    let request = request("attributes.py", output("attributes"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);

    let symbols = Command::new("nm")
        .arg(&artifact.executable)
        .output()
        .unwrap();
    assert!(symbols.status.success());
    let symbols = String::from_utf8_lossy(&symbols.stdout);
    for forbidden in ["Py_", "PyObject", "setjmp", "longjmp", "rimera_compat"] {
        assert!(
            !symbols.contains(forbidden),
            "attribute artifact contains forbidden symbol `{forbidden}`"
        );
    }
}

#[test]
fn missing_attributes_render_cpython_shaped_failures() {
    let request = request("attribute_missing.py", output("attribute_missing"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert!(
        String::from_utf8_lossy(&native.stderr)
            .ends_with("AttributeError: 'Empty' object has no attribute 'missing'\n")
    );
    assert!(
        String::from_utf8_lossy(&python.stderr)
            .ends_with("AttributeError: 'Empty' object has no attribute 'missing'\n")
    );
}

#[test]
fn inheritance_c3_and_explicit_super_match_cpython() {
    let request = request("inheritance.py", output("inheritance"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);

    let symbols = Command::new("nm")
        .arg(&artifact.executable)
        .output()
        .unwrap();
    assert!(symbols.status.success());
    let symbols = String::from_utf8_lossy(&symbols.stdout);
    for forbidden in ["Py_", "PyObject", "setjmp", "longjmp", "rimera_compat"] {
        assert!(
            !symbols.contains(forbidden),
            "inheritance artifact contains forbidden symbol `{forbidden}`"
        );
    }
}

#[test]
fn descriptor_decorators_follow_the_native_object_protocol() {
    let request = request("descriptors.py", output("descriptors"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn slots_use_member_descriptors_and_preserve_inherited_layouts() {
    let request = request("slots.py", output("slots"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn zero_argument_super_uses_the_native_class_closure_cell() {
    let request = request("super_zero_native.py", output("super_zero_native"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn class_decorators_use_the_generic_native_call_path() {
    let request = request("class_decorator.py", output("class_decorator"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn class_body_conditionals_execute_through_native_namespace_operations() {
    let request = request("class_body_if.py", output("class_body_if"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn class_body_local_reads_use_the_prepared_native_namespace() {
    let request = request("class_body_locals.py", output("class_body_locals"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn class_body_loops_execute_through_native_namespace_operations() {
    let request = request("class_body_while.py", output("class_body_while"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn class_body_for_loops_execute_through_native_namespace_operations() {
    let request = request("class_body_for.py", output("class_body_for"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn class_body_name_deletion_uses_the_native_namespace_abi() {
    let request = request("class_body_delete.py", output("class_body_delete"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn extended_binary_operators_use_the_shared_native_protocol_identifiers() {
    let request = request("extended_operators.py", output("extended_operators"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn name_deletion_uses_native_global_and_cell_clear_operations() {
    let request = request("name_delete.py", output("name_delete"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn class_body_loop_control_uses_native_loop_targets() {
    let request = request(
        "class_body_loop_control.py",
        output("class_body_loop_control"),
    );
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn class_body_item_and_attribute_mutation_use_native_operations() {
    let request = request("class_body_mutation.py", output("class_body_mutation"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn class_body_raise_propagates_through_the_native_exception_path() {
    let request = request("class_body_raise.py", output("class_body_raise"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    let native_stderr = String::from_utf8_lossy(&native.stderr);
    let python_stderr = String::from_utf8_lossy(&python.stderr);
    assert!(native_stderr.contains("ValueError: class body failure"));
    assert!(python_stderr.contains("ValueError: class body failure"));
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn class_body_try_except_uses_native_exception_edges_and_namespace_cleanup() {
    let request = request("class_body_try.py", output("class_body_try"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn class_body_finally_uses_native_exception_and_normal_cleanup_edges() {
    let request = request(
        "class_body_try_finally.py",
        output("class_body_try_finally"),
    );
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn class_body_finally_runs_before_native_loop_control_exits() {
    let request = request(
        "class_body_finally_loop_control.py",
        output("class_body_finally_loop_control"),
    );
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn class_body_handler_bindings_are_cleaned_before_native_loop_exits() {
    let request = request(
        "class_body_handler_loop_exit.py",
        output("class_body_handler_loop_exit"),
    );
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn class_body_conditional_methods_use_the_prepared_namespace() {
    let request = request(
        "class_body_nested_method.py",
        output("class_body_nested_method"),
    );
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn nested_class_definitions_publish_through_the_outer_native_namespace() {
    let request = request(
        "class_body_nested_class.py",
        output("class_body_nested_class"),
    );
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn class_bases_assignment_recomputes_native_mro_before_publication() {
    let request = request(
        "class_bases_assignment.py",
        output("class_bases_assignment"),
    );
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn supported_builtin_storage_subclasses_keep_user_type_identity() {
    let request = request(
        "builtin_storage_subclass.py",
        output("builtin_storage_subclass"),
    );
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn builtin_storage_subclass_payloads_use_native_collection_operations() {
    let request = request(
        "builtin_storage_layouts.py",
        output("builtin_storage_layouts"),
    );
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn boolean_operators_preserve_values_and_short_circuit_through_native_cfg() {
    let request = request("boolean_short_circuit.py", output("boolean_short_circuit"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn class_body_expressions_read_the_native_namespace() {
    let request = request("class_body_expression.py", output("class_body_expression"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn descriptor_set_name_hooks_run_during_native_class_construction() {
    let request = request("set_name.py", output("set_name"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn init_subclass_hooks_run_after_native_type_creation() {
    let request = request("init_subclass.py", output("init_subclass"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn dynamic_user_metaclasses_preserve_native_type_identity() {
    let request = request("metaclass_dynamic.py", output("metaclass_dynamic"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn user_metaclass_new_and_init_hooks_use_generic_native_calls() {
    let request = request("metaclass_hooks.py", output("metaclass_hooks"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn prepared_mapping_objects_flow_through_native_class_body_and_type_creation() {
    let request = request(
        "metaclass_prepared_mapping.py",
        output("metaclass_prepared_mapping"),
    );
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn class_keywords_reach_prepare_and_metaclass_constructor_in_source_order() {
    let request = request("metaclass_keywords.py", output("metaclass_keywords"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn universal_protocol_dispatch_uses_native_inplace_items_identity_and_builtins() {
    let request = request("gate2_protocols.py", output("gate2_protocols"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn implicit_metaclass_selection_uses_the_most_derived_compatible_type() {
    let request = request("metaclass_selection.py", output("metaclass_selection"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn metaclass_data_descriptors_precede_class_namespace_values() {
    let request = request("metaclass_descriptor.py", output("metaclass_descriptor"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn custom_attribute_hooks_use_generic_compiled_method_calls() {
    let request = request("attribute_hooks.py", output("attribute_hooks"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn non_type_bases_resolve_through_mro_entries() {
    let request = request("mro_entries.py", output("mro_entries"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn binary_dunders_and_not_implemented_use_generic_dispatch() {
    let request = request("dunder_binary.py", output("dunder_binary"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn comparison_dunders_and_not_implemented_use_generic_dispatch() {
    let request = request("dunder_compare.py", output("dunder_compare"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn unary_dunders_use_generic_dispatch() {
    let request = request("dunder_unary.py", output("dunder_unary"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn positive_and_invert_unary_dunders_use_generic_dispatch() {
    let request = request("dunder_unary_extended.py", output("dunder_unary_extended"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn native_attribute_helper_builtins_use_the_generic_call_path() {
    let request = request("reflection_helpers.py", output("reflection_helpers"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn augmented_assignment_reuses_native_binary_operations_and_bindings() {
    let request = request("augmented_assignment.py", output("augmented_assignment"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn builtin_bool_int_and_str_constructors_use_generic_type_calls() {
    let request = request("builtin_constructors.py", output("builtin_constructors"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn builtin_names_obey_normal_rebinding_and_aliasing_rules() {
    let request = request("builtin_rebinding.py", output("builtin_rebinding"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate3_memoryview_surface_matches_cpython_312() {
    let request = request("gate3_memoryview.py", output("gate3_memoryview"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate3_core_builtins_match_cpython_312() {
    let request = request("gate3_core.py", output("gate3_core"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate3_structured_exceptions_and_complex_match_cpython_312() {
    let request = request(
        "gate3_exceptions_complex.py",
        output("gate3_exceptions_complex"),
    );
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate3_numeric_hash_and_collision_semantics_match_cpython_312() {
    let request = request(
        "gate3_numeric_collections.py",
        output("gate3_numeric_collections"),
    );
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate3_constructor_and_builtin_subclass_semantics_match_cpython_312() {
    let request = request(
        "gate3_constructors_subclasses.py",
        output("gate3_constructors_subclasses"),
    );
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate3_repr_ascii_and_format_semantics_match_cpython_312() {
    let request = request("gate3_repr_format.py", output("gate3_repr_format"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate3_round_and_float_divmod_semantics_match_cpython_312() {
    let request = request("gate3_round_divmod.py", output("gate3_round_divmod"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate3_reversed_and_range_semantics_match_cpython_312() {
    let request = request("gate3_reverse_range.py", output("gate3_reverse_range"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate3_slice_and_memoryview_remaining_semantics_match_cpython_312() {
    let request = request(
        "gate3_slice_memoryview_remaining.py",
        output("gate3_slice_memoryview_remaining"),
    );
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate3_dictionary_view_finishing_matches_cpython_312() {
    let request = request(
        "gate3_dict_views_finish.py",
        output("gate3_dict_views_finish"),
    );
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate3_unicode_scalar_domain_matches_cpython_312() {
    let request = request("gate3_unicode_scalars.py", output("gate3_unicode_scalars"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate3_lone_surrogate_creation_has_an_explicit_runtime_boundary() {
    let request = request(
        "gate3_unicode_surrogate_boundary.py",
        output("gate3_unicode_surrogate_boundary"),
    );
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();

    assert_eq!(python.status.code(), Some(0));
    assert_eq!(python.stdout, b"55296\n");
    assert!(python.stderr.is_empty());
    assert_eq!(native.status.code(), Some(1));
    assert!(native.stdout.is_empty());
    assert!(String::from_utf8_lossy(&native.stderr).ends_with(
        "ValueError: Rimera Gate 3 strings do not support lone surrogate code points\n"
    ));
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate3_builtin_namespace_final_audit_matches_cpython_312() {
    let request = request(
        "gate3_builtin_namespace.py",
        output("gate3_builtin_namespace"),
    );
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate3_small_builtin_type_surfaces_match_cpython_312() {
    let request = request(
        "gate3_small_builtin_surfaces.py",
        output("gate3_small_builtin_surfaces"),
    );
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate3_minimal_regressions_match_cpython_312() {
    let request = request(
        "gate3_minimal_regressions.py",
        output("gate3_minimal_regressions"),
    );
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn truth_length_item_and_contains_dunders_use_generic_dispatch() {
    let request = request("dunder_protocols.py", output("dunder_protocols"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn callable_and_iterable_instances_use_generic_protocol_lookup() {
    let request = request("call_iter_protocols.py", output("call_iter_protocols"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn user_defined_iterators_use_iter_and_next_protocols() {
    let request = request("user_iterator.py", output("user_iterator"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn float_literals_and_arithmetic_use_the_native_builtin_family() {
    let request = request("floats.py", output("floats"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn bytes_complex_and_slice_literals_reach_the_native_runtime() {
    let request = request("builtin_literals.py", output("builtin_literals"));
    let artifact = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn invalid_inheritance_reports_cpython_shaped_type_errors() {
    for (fixture, expected) in [
        (
            "inheritance_duplicate.py",
            "TypeError: duplicate base class Base\n",
        ),
        (
            "inheritance_mro_error.py",
            "TypeError: Cannot create a consistent method resolution\norder (MRO) for bases X, Y\n",
        ),
        (
            "type_class_invalid_base.py",
            "TypeError: metaclass conflict: the metaclass of a derived class must be a (non-strict) subclass of the metaclasses of all its bases\n",
        ),
    ] {
        let request = request(fixture, output(fixture));
        let artifact = rimera_compiler::build(request.clone()).unwrap();
        let native = run(&artifact.executable);
        let python = Command::new("/opt/homebrew/bin/python3.12")
            .arg(&request.entry)
            .output()
            .unwrap();
        assert_eq!(native.status.code(), python.status.code());
        assert_eq!(native.stdout, python.stdout);
        assert!(
            String::from_utf8_lossy(&native.stderr).ends_with(expected),
            "{}",
            String::from_utf8_lossy(&native.stderr)
        );
        assert!(String::from_utf8_lossy(&python.stderr).ends_with(expected));
    }
}

#[test]
fn unsupported_class_forms_are_rejected_before_artifact_output() {
    for fixture in ["class_builtin_base.py", "class_dynamic_base_expression.py"] {
        let path = output(fixture);
        let diagnostics = rimera_compiler::build(request(fixture, path.clone())).unwrap_err();
        assert!(diagnostics.to_string().contains("RIM-CAP-001"));
        assert!(!path.exists());
    }
}

#[test]
fn large_modules_are_outlined_without_changing_order_or_tracebacks() {
    let project_root = std::env::temp_dir().join(format!(
        "rimera-large-module-project-{}",
        std::process::id()
    ));
    fs::create_dir_all(&project_root).unwrap();
    let entry = project_root.join("large.py");
    let mut source = String::from("value = 0\n");
    for index in 0..700 {
        source.push_str(&format!("value = value + {}\n", index % 17));
    }
    source.push_str("print(value)\n");
    fs::write(&entry, &source).unwrap();
    let artifact_output = output("large_module");
    let mut build_request = request("hello.py", artifact_output);
    build_request.project_root = project_root.clone();
    build_request.entry = entry.clone();
    let artifact = rimera_compiler::build(build_request).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&entry)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);

    source.push_str("failure = 1 // 0\n");
    fs::write(&entry, source).unwrap();
    let mut request = request("hello.py", output("large_module_failure"));
    request.project_root = project_root;
    request.entry = entry;
    let artifact = rimera_compiler::build(request).unwrap();
    let failure = run(&artifact.executable);
    assert_eq!(failure.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&failure.stderr);
    assert_eq!(stderr.matches("in <module>").count(), 1, "{stderr}");
    assert!(stderr.contains("line 703, in <module>"), "{stderr}");
}

#[test]
fn source_modules_link_as_native_objects_with_isolated_globals_and_one_time_initialization() {
    ensure_runtime_archive();
    let project_root = workspace().join("tests/fixtures/modules/gate9_native_modules");
    let entry = project_root.join("app.py");
    let executable = output("gate9_native_modules");
    let artifact = rimera_compiler::build(BuildRequest {
        project_root: project_root.clone(),
        module_search_roots: Vec::new(),
        entry: entry.clone(),
        output: executable,
        target: TargetTriple::default(),
        profile: BuildProfile::Debug,
        capabilities: CapabilitySet::default(),
        debug: false,
        heap_limit_bytes: None,
        async_backend: AsyncBackend::Auto,
    })
    .unwrap();

    let native = Command::new(&artifact.executable)
        .current_dir(&project_root)
        .output()
        .unwrap();
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&entry)
        .current_dir(&project_root)
        .output()
        .unwrap();

    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_eq!(
        String::from_utf8_lossy(&native.stdout)
            .matches("shared initialized")
            .count(),
        1
    );
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn module_cache_preserves_cycle_identity_and_rolls_back_failed_initialization() {
    ensure_runtime_archive();
    let project_root = workspace().join("tests/fixtures/modules/gate9_cache_cycles");
    let entry = project_root.join("app.py");
    let artifact = rimera_compiler::build(BuildRequest {
        project_root: project_root.clone(),
        module_search_roots: Vec::new(),
        entry: entry.clone(),
        output: output("gate9_cache_cycles"),
        target: TargetTriple::default(),
        profile: BuildProfile::Debug,
        capabilities: CapabilitySet::default(),
        debug: false,
        heap_limit_bytes: Some(256 * 1024),
        async_backend: AsyncBackend::Auto,
    })
    .unwrap();

    let native = Command::new(&artifact.executable)
        .current_dir(&project_root)
        .output()
        .unwrap();
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&entry)
        .current_dir(&project_root)
        .output()
        .unwrap();

    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn dotted_imports_bind_parent_or_explicit_leaf_across_scopes() {
    ensure_runtime_archive();
    let root = workspace().join("tests/fixtures/modules/gate9_dotted");
    let mut request = request("hello.py", output("gate9_dotted"));
    request.project_root = root.clone();
    request.entry = root.join("app.py");
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    let artifact = rimera_compiler::build(request).unwrap();
    let native = run(&artifact.executable);
    assert_eq!(
        native.status.code(),
        python.status.code(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn from_imports_star_all_submodule_fallback_and_partial_binding_match_cpython() {
    ensure_runtime_archive();
    let root = workspace().join("tests/fixtures/modules/gate9_from_imports");
    let mut request = request("hello.py", output("gate9_from_imports"));
    request.project_root = root.clone();
    request.entry = root.join("app.py");
    request.heap_limit_bytes = Some(512 * 1024);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    let artifact = rimera_compiler::build(request).unwrap();
    let native = run(&artifact.executable);
    assert_eq!(
        native.status.code(),
        python.status.code(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn star_import_rejects_iterator_only_all_like_cpython() {
    ensure_runtime_archive();
    let root = workspace().join("tests/fixtures/modules/gate9_from_imports");
    let mut request = request("hello.py", output("gate9_bad_all"));
    request.project_root = root.clone();
    request.entry = root.join("bad_all_app.py");
    request.heap_limit_bytes = Some(256 * 1024);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    let artifact = rimera_compiler::build(request).unwrap();
    let native = run(&artifact.executable);
    assert_eq!(native.status.code(), python.status.code());
    assert!(
        String::from_utf8_lossy(&native.stderr).contains("does not support indexing"),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert!(
        String::from_utf8_lossy(&python.stderr).contains("does not support indexing"),
        "{}",
        String::from_utf8_lossy(&python.stderr)
    );
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn star_import_preserves_partial_bindings_and_failure_types() {
    ensure_runtime_archive();
    let root = workspace().join("tests/fixtures/modules/gate9_from_imports");
    let mut request = request("hello.py", output("gate9_partial_all"));
    request.project_root = root.clone();
    request.entry = root.join("partial_all_app.py");
    request.heap_limit_bytes = Some(256 * 1024);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    let artifact = rimera_compiler::build(request).unwrap();
    let native = run(&artifact.executable);
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn star_import_is_rejected_outside_module_scope_before_output() {
    ensure_runtime_archive();
    let root = workspace().join("tests/fixtures/modules/gate9_from_imports");
    let output = output("gate9_illegal_star");
    let mut request = request("hello.py", output.clone());
    request.project_root = root.clone();
    request.entry = root.join("illegal_star.py");
    let diagnostics = rimera_compiler::build(request).unwrap_err();
    let diagnostic = &diagnostics.as_slice()[0];
    assert_eq!(diagnostic.code, "RIM-SEMA-001");
    assert!(
        diagnostic
            .message
            .contains("import * only allowed at module level")
    );
    assert!(diagnostic.span.end > diagnostic.span.start);
    assert!(!output.exists());
}

#[test]
fn regular_packages_relative_imports_and_metadata_match_cpython() {
    ensure_runtime_archive();
    let root = workspace().join("tests/fixtures/modules/gate9_packages");
    let mut request = request("hello.py", output("gate9_packages"));
    request.project_root = root.clone();
    request.entry = root.join("app.py");
    request.heap_limit_bytes = Some(512 * 1024);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&request.entry)
        .output()
        .unwrap();
    let artifact = rimera_compiler::build(request).unwrap();
    let native = run(&artifact.executable);
    assert_eq!(
        native.status.code(),
        python.status.code(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn relative_import_beyond_top_level_fails_before_artifact_output() {
    ensure_runtime_archive();
    let root = workspace().join("tests/fixtures/modules/gate9_packages");
    let output = output("gate9_relative_beyond");
    let mut request = request("hello.py", output.clone());
    request.project_root = root.clone();
    request.entry = root.join("beyond_app.py");
    let diagnostics = rimera_compiler::build(request).unwrap_err();
    let diagnostic = &diagnostics.as_slice()[0];
    assert_eq!(diagnostic.code, "RIM-IMPORT-003");
    assert!(
        diagnostic
            .message
            .contains("relative import beyond top-level package")
    );
    assert!(diagnostic.span.end > diagnostic.span.start);
    assert!(!output.exists());
}

#[test]
fn namespace_packages_merge_declared_roots_and_publish_children() {
    ensure_runtime_archive();
    let root = workspace().join("tests/fixtures/modules/gate9_namespaces");
    let entry = root.join("app.py");
    let mut request = request("hello.py", output("gate9_namespaces"));
    request.project_root = root.clone();
    request.module_search_roots = vec![PathBuf::from("root_a"), PathBuf::from("root_b")];
    request.entry = entry.clone();
    request.heap_limit_bytes = Some(512 * 1024);

    let python_path = [root.join("root_a"), root.join("root_b")]
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(":");
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&entry)
        .env("PYTHONPATH", python_path)
        .current_dir(&root)
        .output()
        .unwrap();
    let artifact = rimera_compiler::build(request).unwrap();
    let native = run(&artifact.executable);
    assert_eq!(
        native.status.code(),
        python.status.code(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn regular_packages_precede_earlier_namespace_portions() {
    ensure_runtime_archive();
    let root = workspace().join("tests/fixtures/modules/gate9_namespace_precedence");
    let entry = root.join("app.py");
    let mut request = request("hello.py", output("gate9_namespace_precedence"));
    request.project_root = root.clone();
    request.module_search_roots = vec![PathBuf::from("root_a"), PathBuf::from("root_b")];
    request.entry = entry.clone();

    let python_path = [root.join("root_a"), root.join("root_b")]
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(":");
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&entry)
        .env("PYTHONPATH", python_path)
        .current_dir(&root)
        .output()
        .unwrap();
    let artifact = rimera_compiler::build(request).unwrap();
    let native = run(&artifact.executable);
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn sys_modules_and_import_builtin_match_cpython_under_gc_pressure() {
    ensure_runtime_archive();
    let root = workspace().join("tests/fixtures/modules/gate9_sys_modules");
    let entry = root.join("app.py");
    let mut request = request("hello.py", output("gate9_sys_modules"));
    request.project_root = root.clone();
    request.entry = entry.clone();
    request.heap_limit_bytes = Some(512 * 1024);

    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&entry)
        .current_dir(&root)
        .output()
        .unwrap();
    let artifact = rimera_compiler::build(request).unwrap();
    let native = run(&artifact.executable);
    assert_eq!(
        native.status.code(),
        python.status.code(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn import_hooks_reload_and_reentrant_loading_match_cpython() {
    ensure_runtime_archive();
    let root = workspace().join("tests/fixtures/modules/gate9_hooks_reload");
    let entry = root.join("app.py");
    let mut request = request("hello.py", output("gate9_hooks_reload"));
    request.project_root = root.clone();
    request.entry = entry.clone();
    request.heap_limit_bytes = Some(512 * 1024);

    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&entry)
        .current_dir(&root)
        .output()
        .unwrap();
    let artifact = rimera_compiler::build(request).unwrap();
    let native = run(&artifact.executable);
    assert_eq!(
        native.status.code(),
        python.status.code(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn locked_package_resources_and_build_manifest_are_reproducible() {
    ensure_runtime_archive();
    let root = workspace().join("tests/fixtures/modules/gate9_locked_resources");
    let entry = root.join("app.py");
    let mut request = request("hello.py", output("gate9_locked_resources"));
    request.project_root = root.clone();
    request.entry = entry.clone();

    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&entry)
        .env("PYTHONWARNINGS", "ignore::DeprecationWarning")
        .current_dir(&root)
        .output()
        .unwrap();
    let first = rimera_compiler::build(request.clone()).unwrap();
    let native = run(&first.executable);
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    let first_manifest = fs::read_to_string(first.manifest.as_ref().unwrap()).unwrap();
    let second = rimera_compiler::build(request).unwrap();
    let second_manifest = fs::read_to_string(second.manifest.as_ref().unwrap()).unwrap();
    assert_eq!(first_manifest, second_manifest);
    for expected in [
        "runtime_abi = 1",
        "name = \"fixture-package\"",
        "name = \"message.txt\"",
        "hash = \"f29e14873b1fdb17\"",
    ] {
        assert!(first_manifest.contains(expected), "{first_manifest}");
    }
    assert_native_only_artifact(&first.executable);
}

#[test]
fn stale_locked_resource_fails_before_artifact_publication() {
    let temporary = std::env::temp_dir().join(format!("rimera-stale-lock-{}", std::process::id()));
    fs::create_dir_all(temporary.join("pkg")).unwrap();
    fs::copy(
        workspace().join("tests/fixtures/modules/gate9_locked_resources/app.py"),
        temporary.join("app.py"),
    )
    .unwrap();
    fs::copy(
        workspace().join("tests/fixtures/modules/gate9_locked_resources/pkg/__init__.py"),
        temporary.join("pkg/__init__.py"),
    )
    .unwrap();
    fs::write(temporary.join("pkg/message.txt"), "changed\n").unwrap();
    fs::write(
        temporary.join("pyproject.toml"),
        "[tool.rimera]\nlocked = true\n",
    )
    .unwrap();
    fs::copy(
        workspace().join("tests/fixtures/modules/gate9_locked_resources/rimera.lock"),
        temporary.join("rimera.lock"),
    )
    .unwrap();
    let output = temporary.join("out");
    let mut request = request("hello.py", output.clone());
    request.project_root = temporary.clone();
    request.entry = temporary.join("app.py");
    let diagnostics = rimera_compiler::build(request).unwrap_err();
    assert_eq!(diagnostics.as_slice()[0].code, "RIM-LOCK-002");
    assert!(!output.exists());
}

#[test]
fn gate9_cross_feature_reentrant_reload_stress_matches_cpython_under_low_heap() {
    ensure_runtime_archive();
    let root = workspace().join("tests/fixtures/modules/gate9_composition_stress");
    let entry = root.join("app.py");
    let mut build_request = request("hello.py", output("gate9_composition_stress"));
    build_request.project_root = root.clone();
    build_request.entry = entry.clone();
    build_request.heap_limit_bytes = Some(64 * 1024);
    let artifact = rimera_compiler::build(build_request).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&entry)
        .current_dir(&root)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate10_slice1_preimplementation_oracles_and_budgets_are_frozen() {
    let result = Command::new("/opt/homebrew/bin/python3.12")
        .current_dir(workspace())
        .arg(workspace().join("scripts/verify_gate10_slice1.py"))
        .output()
        .expect("run Gate 10 Slice 1 evidence verifier");
    assert!(
        result.status.success(),
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&result.stdout),
        "Gate 10 Slice 1 preparatory evidence verified\n"
    );
}

#[test]
fn gate10_slice2_async_await_uses_explicit_native_suspension_and_no_artifact_negatives() {
    ensure_runtime_archive();
    let root = workspace().join("tests/fixtures/async");
    let entry = root.join("gate10_slice23_protocol.py");
    let mut build_request = request("hello.py", output("gate10_slice2_suspend"));
    build_request.project_root = root.clone();
    build_request.entry = entry.clone();
    build_request.debug = true;

    let artifact = rimera_compiler::build(build_request).unwrap();
    let dump = fs::read_to_string(artifact.ir_dump.as_ref().expect("debug MIR dump")).unwrap();
    assert!(dump.contains("suspend Await"), "{dump}");
    assert!(
        !dump.contains("# yield ValueId"),
        "coroutine await must not lower as generator yield: {dump}"
    );
    assert_native_only_artifact(&artifact.executable);

    let temporary =
        std::env::temp_dir().join(format!("rimera-gate10-slice2-{}", std::process::id()));
    fs::create_dir_all(&temporary).unwrap();
    for (index, (source, expected)) in [
        ("await value\n", "'await' outside function"),
        (
            "def f():\n    return await value\n",
            "'await' outside async function",
        ),
        (
            "async for item in values:\n    value = item\n",
            "'async for' outside async function",
        ),
        (
            "async with manager:\n    value = 1\n",
            "'async with' outside async function",
        ),
        (
            "values = [x async for x in source]\n",
            "asynchronous comprehension outside of an asynchronous function",
        ),
        (
            "async def f():\n    yield from source\n",
            "'yield from' inside async function",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let entry = temporary.join(format!("negative_{index}.py"));
        fs::write(&entry, source).unwrap();
        let executable = temporary.join(format!("negative_{index}"));
        let mut build_request = request("hello.py", executable.clone());
        build_request.project_root = temporary.clone();
        build_request.entry = entry;
        let diagnostics = rimera_compiler::build(build_request).unwrap_err();
        assert!(
            diagnostics
                .as_slice()
                .iter()
                .any(|diagnostic| diagnostic.message == expected),
            "expected {expected:?}, got {diagnostics:?}"
        );
        assert!(
            !executable.exists(),
            "negative async source published an artifact"
        );
    }
}

#[test]
fn gate10_slice3_native_coroutine_protocol_matches_cpython_and_warns_when_unawaited() {
    ensure_runtime_archive();
    let root = workspace().join("tests/fixtures/async");
    let entry = root.join("gate10_slice23_protocol.py");
    let mut build_request = request("hello.py", output("gate10_slice3_protocol"));
    build_request.project_root = root.clone();
    build_request.entry = entry.clone();
    let artifact = rimera_compiler::build(build_request).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&entry)
        .current_dir(&root)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&artifact.executable);

    let warning_entry = root.join("gate10_slice3_unawaited.py");
    let mut warning_request = request("hello.py", output("gate10_slice3_unawaited"));
    warning_request.project_root = root.clone();
    warning_request.entry = warning_entry;
    let warning_artifact = rimera_compiler::build(warning_request).unwrap();
    let warning = run(&warning_artifact.executable);
    assert_eq!(warning.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&warning.stderr);
    assert!(
        stderr.contains("RuntimeWarning: coroutine 'pending' was never awaited"),
        "{stderr}"
    );
    assert_native_only_artifact(&warning_artifact.executable);

    let low_heap_entry = root.join("gate10_slice3_low_heap.py");
    let mut low_heap_request = request("hello.py", output("gate10_slice3_low_heap"));
    low_heap_request.project_root = root.clone();
    low_heap_request.entry = low_heap_entry.clone();
    low_heap_request.heap_limit_bytes = Some(16 * 1024);
    let low_heap_artifact = rimera_compiler::build(low_heap_request).unwrap();
    let native = run(&low_heap_artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(low_heap_entry)
        .current_dir(&root)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&low_heap_artifact.executable);
}

#[test]
fn gate10_slice4_5_sync_control_has_zero_async_backend_reachability() {
    ensure_runtime_archive();
    let artifact =
        rimera_compiler::build(request("hello.py", output("gate10_slice45_sync_control"))).unwrap();
    let native = run(&artifact.executable);
    assert_eq!(native.status.code(), Some(0));
    assert_native_only_artifact(&artifact.executable);
    assert_no_async_backend_symbols(&artifact.executable);
}

#[test]
fn gate10_slice6_generic_awaitable_protocol_matches_cpython() {
    ensure_runtime_archive();
    let root = workspace().join("tests/fixtures/async");
    let entry = root.join("gate10_slice6_awaitables.py");
    let mut build_request = request("hello.py", output("gate10_slice6_awaitables"));
    build_request.project_root = root.clone();
    build_request.entry = entry.clone();
    build_request.debug = true;

    let artifact = rimera_compiler::build(build_request).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&entry)
        .current_dir(&root)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    let dump = fs::read_to_string(artifact.ir_dump.as_ref().expect("debug MIR dump")).unwrap();
    assert!(dump.contains("await_iter("), "{dump}");
    assert!(dump.contains("yield_from_next("), "{dump}");
    assert_native_only_artifact(&artifact.executable);

    let mut low_heap_request = request("hello.py", output("gate10_slice6_awaitables_low_heap"));
    low_heap_request.project_root = root.clone();
    low_heap_request.entry = entry.clone();
    low_heap_request.heap_limit_bytes = Some(64 * 1024);
    let low_heap_artifact = rimera_compiler::build(low_heap_request).unwrap();
    let low_heap = run(&low_heap_artifact.executable);
    assert_eq!(low_heap.status.code(), python.status.code());
    assert_eq!(low_heap.stdout, python.stdout);
    assert_eq!(low_heap.stderr, python.stderr);
    assert_native_only_artifact(&low_heap_artifact.executable);
}

#[test]
fn gate10_slice7_async_for_matches_cpython() {
    ensure_runtime_archive();
    let root = workspace().join("tests/fixtures/async");
    let entry = root.join("gate10_slice7_async_for.py");
    let mut build_request = request("hello.py", output("gate10_slice7_async_for"));
    build_request.project_root = root.clone();
    build_request.entry = entry.clone();
    build_request.debug = true;

    let artifact = rimera_compiler::build(build_request).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&entry)
        .current_dir(&root)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    let dump = fs::read_to_string(artifact.ir_dump.as_ref().expect("debug MIR dump")).unwrap();
    assert!(dump.contains("async_iter("), "{dump}");
    assert!(dump.contains("async_next("), "{dump}");
    assert!(dump.contains("await_iter("), "{dump}");
    assert_native_only_artifact(&artifact.executable);

    let mut low_heap_request = request("hello.py", output("gate10_slice7_async_for_low_heap"));
    low_heap_request.project_root = root.clone();
    low_heap_request.entry = entry.clone();
    low_heap_request.heap_limit_bytes = Some(64 * 1024);
    let low_heap_artifact = rimera_compiler::build(low_heap_request).unwrap();
    let low_heap = run(&low_heap_artifact.executable);
    assert_eq!(low_heap.status.code(), python.status.code());
    assert_eq!(low_heap.stdout, python.stdout);
    assert_eq!(low_heap.stderr, python.stderr);
    assert_native_only_artifact(&low_heap_artifact.executable);
}

#[test]
fn gate10_slice7_async_comprehensions_match_cpython() {
    ensure_runtime_archive();
    let root = workspace().join("tests/fixtures/async");
    let entry = root.join("gate10_slice7_async_comprehensions.py");
    let mut build_request = request("hello.py", output("gate10_slice7_async_comprehensions"));
    build_request.project_root = root.clone();
    build_request.entry = entry.clone();
    build_request.debug = true;

    let artifact = rimera_compiler::build(build_request).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&entry)
        .current_dir(&root)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    let dump = fs::read_to_string(artifact.ir_dump.as_ref().expect("debug MIR dump")).unwrap();
    assert!(dump.contains("async_iter("), "{dump}");
    assert!(dump.contains("async_next("), "{dump}");
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate10_slice8_async_generator_protocol_matches_cpython() {
    ensure_runtime_archive();
    let root = workspace().join("tests/fixtures/async");
    let entry = root.join("gate10_slice8_async_generators.py");
    let mut build_request = request("hello.py", output("gate10_slice8_async_generators"));
    build_request.project_root = root.clone();
    build_request.entry = entry.clone();
    build_request.debug = true;

    let artifact = rimera_compiler::build(build_request).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&entry)
        .current_dir(&root)
        .output()
        .unwrap();
    assert_eq!(
        native.status.code(),
        python.status.code(),
        "native stdout:\n{}\nnative stderr:\n{}\npython stdout:\n{}\npython stderr:\n{}",
        String::from_utf8_lossy(&native.stdout),
        String::from_utf8_lossy(&native.stderr),
        String::from_utf8_lossy(&python.stdout),
        String::from_utf8_lossy(&python.stderr),
    );
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    let dump = fs::read_to_string(artifact.ir_dump.as_ref().expect("debug MIR dump")).unwrap();
    assert!(dump.contains("await_iter("), "{dump}");
    let source = fs::read_to_string(&entry).unwrap();
    let syntax = rimera_compiler::syntax::parse(&entry, &source).unwrap();
    let hir = rimera_compiler::sema::analyze(&entry, &syntax).unwrap();
    let program = rimera_compiler::lower::lower(&hir).unwrap();
    let basic = program
        .functions
        .iter()
        .find(|function| function.name == "basic")
        .expect("basic async generator MIR function");
    assert_eq!(
        basic.kind,
        rimera_compiler::mir::FunctionKind::AsyncGenerator
    );
    assert!(
        basic.blocks.iter().any(|block| matches!(
            block.terminator,
            rimera_compiler::mir::Terminator::Yield { .. }
        )),
        "async generator body must lower yields into the suspended native function"
    );
    assert_native_only_artifact(&artifact.executable);

    let mut low_heap_request = request(
        "hello.py",
        output("gate10_slice8_async_generators_low_heap"),
    );
    low_heap_request.project_root = root.clone();
    low_heap_request.entry = entry.clone();
    low_heap_request.heap_limit_bytes = Some(96 * 1024);
    let low_heap_artifact = rimera_compiler::build(low_heap_request).unwrap();
    let low_heap = run(&low_heap_artifact.executable);
    assert_eq!(low_heap.status.code(), python.status.code());
    assert_eq!(low_heap.stdout, python.stdout);
    assert_eq!(low_heap.stderr, python.stderr);
    assert_native_only_artifact(&low_heap_artifact.executable);
}

#[test]
fn gate10_slice8_abandoned_async_generator_cycle_finalizes_exactly_once() {
    ensure_runtime_archive();
    let root = workspace().join("tests/fixtures/async");
    let entry = root.join("gate10_slice8_finalization.py");
    let mut build_request = request("hello.py", output("gate10_slice8_finalization"));
    build_request.project_root = root.clone();
    build_request.entry = entry.clone();

    let artifact = rimera_compiler::build(build_request).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&entry)
        .current_dir(&root)
        .output()
        .unwrap();
    assert_eq!(native.status.code(), python.status.code());
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_eq!(
        String::from_utf8_lossy(&native.stdout),
        "started 1\n['cycle-finally']\nshutdown-started 2\nbefore-shutdown\nshutdown-finally\n"
    );
    assert_native_only_artifact(&artifact.executable);
}

#[test]
fn gate10_slice8_async_generator_return_value_is_rejected_before_artifact_output() {
    let temporary = std::env::temp_dir().join(format!(
        "rimera-gate10-slice8-return-{}",
        std::process::id()
    ));
    fs::create_dir_all(&temporary).unwrap();
    let entry = temporary.join("invalid.py");
    fs::write(&entry, "async def invalid():\n    yield 1\n    return 2\n").unwrap();
    let executable = temporary.join("invalid");
    let mut build_request = request("hello.py", executable.clone());
    build_request.project_root = temporary;
    build_request.entry = entry;
    let diagnostics = rimera_compiler::build(build_request).unwrap_err();
    assert!(
        diagnostics
            .as_slice()
            .iter()
            .any(|diagnostic| diagnostic.message == "'return' with value in async generator"),
        "unexpected diagnostics: {diagnostics:?}"
    );
    assert!(
        !executable.exists(),
        "invalid async generator published an artifact"
    );
}

#[test]
fn gate10_slice9_async_with_cleanup_matches_cpython() {
    ensure_runtime_archive();
    let root = workspace().join("tests/fixtures/async");
    let entry = root.join("gate10_slice9_async_with.py");
    let mut build_request = request("hello.py", output("gate10_slice9_async_with"));
    build_request.project_root = root.clone();
    build_request.entry = entry.clone();
    build_request.debug = true;

    let artifact = rimera_compiler::build(build_request).unwrap();
    let native = run(&artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&entry)
        .current_dir(&root)
        .output()
        .unwrap();
    assert_eq!(
        native.status.code(),
        python.status.code(),
        "native stdout:\n{}\nnative stderr:\n{}\npython stdout:\n{}\npython stderr:\n{}",
        String::from_utf8_lossy(&native.stdout),
        String::from_utf8_lossy(&native.stderr),
        String::from_utf8_lossy(&python.stdout),
        String::from_utf8_lossy(&python.stderr),
    );
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    let dump = fs::read_to_string(artifact.ir_dump.as_ref().expect("debug MIR dump")).unwrap();
    assert!(
        dump.contains("special_method_get") || dump.contains("__aenter__"),
        "{dump}"
    );
    assert!(dump.contains("await_iter("), "{dump}");
    assert_native_only_artifact(&artifact.executable);

    let mut low_heap_request = request("hello.py", output("gate10_slice9_async_with_low_heap"));
    low_heap_request.project_root = root.clone();
    low_heap_request.entry = entry.clone();
    low_heap_request.heap_limit_bytes = Some(128 * 1024);
    let low_heap_artifact = rimera_compiler::build(low_heap_request).unwrap();
    let low_heap = run(&low_heap_artifact.executable);
    assert_eq!(low_heap.status.code(), python.status.code());
    assert_eq!(low_heap.stdout, python.stdout);
    assert_eq!(low_heap.stderr, python.stderr);
    assert_native_only_artifact(&low_heap_artifact.executable);
}

#[test]
fn gate10_slice10_compio_root_is_linked_only_when_async_execution_is_reachable() {
    ensure_runtime_archive();
    let mut async_request = isolated_async_request(
        "gate10-slice10-auto",
        "gate10_slice10_compio_root.py",
        "gate10_slice10_compio_root",
    );
    async_request.async_backend = AsyncBackend::Auto;

    let async_artifact = rimera_compiler::build(async_request).unwrap();
    assert_eq!(async_artifact.async_backend, Some(AsyncBackend::Compio));
    let native = run(&async_artifact.executable);
    assert_eq!(native.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&native.stdout),
        "async-root 42\nroot-return 42\n"
    );
    assert!(
        native.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_native_only_artifact(&async_artifact.executable);
    let symbols = Command::new("nm")
        .arg(&async_artifact.executable)
        .output()
        .unwrap();
    assert!(symbols.status.success());
    let symbols = String::from_utf8_lossy(&symbols.stdout).to_ascii_lowercase();
    assert!(
        symbols.contains("rimera_async_backend_select_compio"),
        "{symbols}"
    );
    assert!(symbols.contains("compio"), "{symbols}");
    assert!(!symbols.contains("monoio"), "{symbols}");
    assert!(!symbols.contains("tokio"), "{symbols}");

    let mut sync_request = isolated_async_request(
        "gate10-slice10-sync",
        "gate10_slice10_sync_control.py",
        "gate10_slice10_sync_control",
    );
    sync_request.async_backend = AsyncBackend::Compio;
    let sync_artifact = rimera_compiler::build(sync_request).unwrap();
    assert_eq!(sync_artifact.async_backend, None);
    assert_no_async_backend_symbols(&sync_artifact.executable);
    assert_native_only_artifact(&sync_artifact.executable);
}

#[test]
fn gate10_slice10_unavailable_backends_fail_before_artifact_output() {
    for backend in [AsyncBackend::Monoio, AsyncBackend::Tokio] {
        let executable = output(&format!("gate10_slice10_unavailable_{}", backend.as_str()));
        let mut build_request = request("hello.py", executable.clone());
        build_request.async_backend = backend;
        let diagnostics = rimera_compiler::build(build_request).unwrap_err();
        let rendered = diagnostics.to_string();
        assert!(rendered.contains("RIM-ASYNC-001"), "{rendered}");
        assert!(rendered.contains(backend.as_str()), "{rendered}");
        assert!(rendered.contains(TargetTriple::MACOS_ARM64), "{rendered}");
        assert!(rendered.contains("available backend: compio"), "{rendered}");
        assert!(
            !executable.exists(),
            "unavailable backend published an artifact"
        );
    }
}

#[test]
fn gate10_slice10_explicit_compio_matches_auto_for_async_root_execution() {
    ensure_runtime_archive();
    let mut build_request = isolated_async_request(
        "gate10-slice10-explicit",
        "gate10_slice10_compio_root.py",
        "gate10_slice10_explicit_compio",
    );
    build_request.async_backend = AsyncBackend::Compio;
    let artifact = rimera_compiler::build(build_request).unwrap();
    assert_eq!(artifact.async_backend, Some(AsyncBackend::Compio));
    let native = run(&artifact.executable);
    assert_eq!(native.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&native.stdout),
        "async-root 42\nroot-return 42\n"
    );
    assert!(
        native.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
}

#[test]
fn gate10_slice10_release_async_size_delta_stays_within_fixed_budget() {
    ensure_release_runtime_archive();
    let mut sync_request = isolated_async_request(
        "gate10-slice10-release-sync",
        "gate10_slice10_sync_control.py",
        "gate10_slice10_release_sync",
    );
    sync_request.profile = BuildProfile::Release;
    let sync_artifact = rimera_compiler::build(sync_request).unwrap();
    assert_eq!(sync_artifact.async_backend, None);
    assert_no_async_backend_symbols(&sync_artifact.executable);

    let mut async_request = isolated_async_request(
        "gate10-slice10-release-async",
        "gate10_slice10_compio_root.py",
        "gate10_slice10_release_async",
    );
    async_request.profile = BuildProfile::Release;
    async_request.async_backend = AsyncBackend::Auto;
    let async_artifact = rimera_compiler::build(async_request).unwrap();
    assert_eq!(async_artifact.async_backend, Some(AsyncBackend::Compio));
    let native = run(&async_artifact.executable);
    assert_eq!(native.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&native.stdout),
        "async-root 42\nroot-return 42\n"
    );

    let sync_bytes = fs::metadata(&sync_artifact.executable).unwrap().len();
    let async_bytes = fs::metadata(&async_artifact.executable).unwrap().len();
    let delta = async_bytes.saturating_sub(sync_bytes);
    assert!(
        delta <= 524_288,
        "async release artifact grew by {delta} bytes ({sync_bytes} -> {async_bytes}), budget is 524288"
    );
}

#[test]
fn gate10_slice11_cross_feature_stress_matches_cpython_under_gc_and_low_heap() {
    ensure_runtime_archive();
    let root = workspace().join("tests/fixtures/async/gate10_slice11");
    let entry = root.join("app.py");
    let mut build_request = request("hello.py", output("gate10_slice11_composition"));
    build_request.project_root = root.clone();
    build_request.entry = entry.clone();
    build_request.async_backend = AsyncBackend::Auto;

    let artifact = rimera_compiler::build(build_request).unwrap();
    assert_eq!(artifact.async_backend, Some(AsyncBackend::Compio));
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(&entry)
        .current_dir(&root)
        .output()
        .unwrap();
    assert_eq!(
        python.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&python.stderr)
    );

    for attempt in 0..5 {
        let native = run(&artifact.executable);
        assert_eq!(
            native.status.code(),
            python.status.code(),
            "attempt {attempt}: native stderr:\n{}\npython stderr:\n{}",
            String::from_utf8_lossy(&native.stderr),
            String::from_utf8_lossy(&python.stderr),
        );
        assert_eq!(native.stdout, python.stdout, "attempt {attempt}");
        assert_eq!(native.stderr, python.stderr, "attempt {attempt}");
    }
    assert_native_only_artifact(&artifact.executable);

    let mut low_heap_request = request("hello.py", output("gate10_slice11_composition_low_heap"));
    low_heap_request.project_root = root.clone();
    low_heap_request.entry = entry;
    low_heap_request.async_backend = AsyncBackend::Auto;
    low_heap_request.heap_limit_bytes = Some(512 * 1024);
    let low_heap_artifact = rimera_compiler::build(low_heap_request).unwrap();
    assert_eq!(low_heap_artifact.async_backend, Some(AsyncBackend::Compio));
    for attempt in 0..3 {
        let native = run(&low_heap_artifact.executable);
        assert_eq!(
            native.status.code(),
            python.status.code(),
            "low-heap attempt {attempt}: native stderr:\n{}\npython stderr:\n{}",
            String::from_utf8_lossy(&native.stderr),
            String::from_utf8_lossy(&python.stderr),
        );
        assert_eq!(native.stdout, python.stdout, "low-heap attempt {attempt}");
        assert_eq!(native.stderr, python.stderr, "low-heap attempt {attempt}");
    }
    assert_native_only_artifact(&low_heap_artifact.executable);
}

#[test]
fn gate10_slice13_creation_close_elision_matches_cpython_and_falls_back() {
    ensure_runtime_archive();
    let fixtures = workspace().join("tests/fixtures/async/gate10_slice13");

    for (project, fixture, output_name) in [
        (
            "gate10-slice13-creation-semantics",
            "gate10_slice13/creation_close_semantics.py",
            "gate10_slice13_creation_semantics",
        ),
        (
            "gate10-slice13-creation-rebound",
            "gate10_slice13/creation_close_rebound.py",
            "gate10_slice13_creation_rebound",
        ),
    ] {
        let request = isolated_async_request(project, fixture, output_name);
        let artifact = rimera_compiler::build(request).unwrap();
        let native = run(&artifact.executable);
        let python = Command::new("/opt/homebrew/bin/python3.12")
            .arg(fixtures.join(Path::new(fixture).file_name().unwrap()))
            .output()
            .unwrap();
        assert_eq!(python.status.code(), Some(0), "{python:?}");
        assert_eq!(native.status.code(), python.status.code(), "{native:?}");
        assert_eq!(native.stdout, python.stdout, "fixture {fixture}");
        assert_eq!(native.stderr, python.stderr, "fixture {fixture}");
        assert_native_only_artifact(&artifact.executable);
    }

    let mut low_heap_request = isolated_async_request(
        "gate10-slice13-creation-lowheap",
        "gate10_slice13/creation_close_semantics.py",
        "gate10_slice13_creation_lowheap",
    );
    low_heap_request.heap_limit_bytes = Some(128 * 1024);
    let low_heap_artifact = rimera_compiler::build(low_heap_request).unwrap();
    let native = run(&low_heap_artifact.executable);
    let python = Command::new("/opt/homebrew/bin/python3.12")
        .arg(fixtures.join("creation_close_semantics.py"))
        .output()
        .unwrap();
    assert_eq!(python.status.code(), Some(0), "{python:?}");
    assert_eq!(native.status.code(), python.status.code(), "{native:?}");
    assert_eq!(native.stdout, python.stdout);
    assert_eq!(native.stderr, python.stderr);
    assert_native_only_artifact(&low_heap_artifact.executable);
}

#[test]
fn gate10_reentrant_root_rejects_a_second_executor_and_restores_the_entry() {
    let request = isolated_async_request(
        "gate10-reentrant-root",
        "gate10_slice11/reentrant_root.py",
        "gate10-reentrant-root",
    );
    let artifact = rimera_compiler::build(request).unwrap();
    let result = run(&artifact.executable);
    assert_eq!(result.status.code(), Some(0), "{result:?}");
    assert_eq!(
        result.stdout,
        b"async_runtime.run() cannot be nested\nouter 41\nroot-failure\nnext 41\n"
    );
    assert!(result.stderr.is_empty(), "{result:?}");
    assert_native_only_artifact(&artifact.executable);
}
