use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use rimera_compiler::core::{BuildProfile, Span, TargetTriple};
use rimera_compiler::mir::{
    Block, BlockId, Constant, Function, FunctionId, Operation, OperationKind, Program, Terminator,
    ValueId,
};
use rimera_compiler::project::{BuildRequest, CapabilitySet};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
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

fn output(name: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("rimera-native-{}-{}", std::process::id(), name));
    fs::create_dir_all(&directory).unwrap();
    directory.join(name)
}

fn request(fixture: &str, output: PathBuf) -> BuildRequest {
    ensure_runtime_archive();
    let root = workspace();
    BuildRequest {
        project_root: root.clone(),
        entry: root.join("tests/fixtures/basic").join(fixture),
        output,
        target: TargetTriple::default(),
        profile: BuildProfile::Debug,
        capabilities: CapabilitySet::default(),
        debug: false,
        heap_limit_bytes: None,
    }
}

fn run(path: &Path) -> std::process::Output {
    Command::new(path).output().unwrap()
}

fn assert_native_only_artifact(path: &Path) {
    let symbols = Command::new("nm").arg(path).output().unwrap();
    assert!(symbols.status.success());
    let symbols = String::from_utf8_lossy(&symbols.stdout);
    for forbidden in ["Py_", "PyObject", "setjmp", "longjmp", "rimera_compat"] {
        assert!(
            !symbols.contains(forbidden),
            "artifact contains forbidden symbol `{forbidden}`"
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
    request.heap_limit_bytes = Some(8192);
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
    request.heap_limit_bytes = Some(8192);
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
        ("unsupported.py", "RIM-CAP-001"),
        ("malformed.py", "RIM-PARSE-001"),
    ] {
        let output = output(fixture);
        let diagnostics = rimera_compiler::build(request(fixture, output.clone())).unwrap_err();
        assert!(diagnostics.to_string().contains(code));
        assert!(!output.exists());
    }
}

#[test]
fn gate4_owned_later_slice_forms_have_stable_capability_diagnostics() {
    for (fixture, code) in [
        ("gate4_cap_multiple_star.py", "RIM-CAP-G4-02"),
        ("gate4_cap_dict_unpack.py", "RIM-CAP-G4-06"),
        ("gate4_cap_expanded_call.py", "RIM-CAP-G4-07"),
        ("gate4_cap_compare_chain.py", "RIM-CAP-G4-09"),
        ("gate4_cap_assert.py", "RIM-CAP-G4-11"),
        ("gate4_cap_annotation.py", "RIM-CAP-G4-13"),
        ("gate4_cap_list_comp.py", "RIM-CAP-G4-16"),
        ("gate4_cap_match.py", "RIM-CAP-G4-19"),
    ] {
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
