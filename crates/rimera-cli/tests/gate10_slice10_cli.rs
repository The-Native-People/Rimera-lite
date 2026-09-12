use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ensure_runtime_archive() {
    let root = workspace();
    let archive = root.join("target/debug/librimera_runtime.a");
    // Cargo's freshness check is necessary: an existing archive can predate
    // the runtime source being exercised by this invocation.
    let status = Command::new("cargo")
        .args(["build", "-p", "rimera-runtime"])
        .current_dir(&root)
        .status()
        .expect("cargo must be available for the CLI integration test");
    assert!(
        status.success(),
        "failed to build the Rimera runtime archive"
    );
    assert!(archive.is_file(), "runtime static archive was not produced");
}

fn run_cli(project: &Path, entry: &Path, output: &Path, extra: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rimera-lite"));
    command
        .current_dir(project)
        .env("RIMERA_COLOR", "never")
        .arg(entry)
        .arg("--output")
        .arg(output);
    command.args(extra);
    command.output().expect("rimera-lite CLI must launch")
}

fn async_source() -> &'static str {
    r#"from rimera import async_runtime

async def child(value):
    return value + 1

async def main():
    value = await child(41)
    print("cli-async", value)
    return value

print("cli-return", async_runtime.run(main()))
"#
}

fn sync_source() -> &'static str {
    r#"def main():
    print("cli-sync", 42)
    return 42

print("cli-return", main())
"#
}

fn assert_execution(artifact: &Path, expected: &str) {
    let result = Command::new(artifact).output().unwrap();
    assert!(result.status.success(), "{result:?}");
    assert_eq!(String::from_utf8_lossy(&result.stdout), expected);
    assert!(result.stderr.is_empty(), "{result:?}");
}

#[test]
fn gate10_slice10_cli_precedence_metadata_and_unavailable_backend_are_public() {
    ensure_runtime_archive();
    let project =
        std::env::temp_dir().join(format!("rimera-gate10-slice10-cli-{}", std::process::id()));
    let _ = fs::remove_dir_all(&project);
    fs::create_dir_all(&project).unwrap();
    let async_entry = project.join("async_main.py");
    let sync_entry = project.join("sync_main.py");
    fs::write(&async_entry, async_source()).unwrap();
    fs::write(&sync_entry, sync_source()).unwrap();

    // A concrete CLI choice overrides an unavailable project selection.
    fs::write(
        project.join("pyproject.toml"),
        "[tool.rimera]\nasync = \"tokio\"\n",
    )
    .unwrap();
    let override_artifact = project.join("override-compio");
    let override_output = run_cli(
        &project,
        &async_entry,
        &override_artifact,
        &["--async", "compio"],
    );
    assert!(
        override_output.status.success(),
        "{}",
        String::from_utf8_lossy(&override_output.stderr)
    );
    let override_stderr = String::from_utf8_lossy(&override_output.stderr);
    assert!(
        override_stderr.contains("async   compio · experimental"),
        "{override_stderr}"
    );
    assert!(override_artifact.is_file());
    assert_execution(&override_artifact, "cli-async 42\ncli-return 42\n");

    // With no CLI override the project setting is authoritative and fails
    // before artifact publication with the stable backend/target diagnostic.
    let unavailable_artifact = project.join("configured-tokio");
    let unavailable = run_cli(&project, &async_entry, &unavailable_artifact, &[]);
    assert!(!unavailable.status.success());
    let unavailable_stderr = String::from_utf8_lossy(&unavailable.stderr);
    let normalized_unavailable = unavailable_stderr
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        unavailable_stderr.contains("RIM-ASYNC-001"),
        "{unavailable_stderr}"
    );
    assert!(unavailable_stderr.contains("tokio"), "{unavailable_stderr}");
    assert!(
        unavailable_stderr.contains("aarch64-apple-darwin"),
        "{unavailable_stderr}"
    );
    assert!(
        normalized_unavailable.contains("available backend: compio"),
        "{unavailable_stderr}"
    );
    assert!(!unavailable_artifact.exists());

    for (name, arguments, expected) in [
        ("monoio", vec!["--async", "monoio"], "RIM-ASYNC-001"),
        (
            "unknown",
            vec!["--async", "unknown"],
            "invalid value 'unknown'",
        ),
        (
            "unsupported-target",
            vec!["--async", "compio", "--target", "x86_64-unknown-linux-gnu"],
            "unsupported target `x86_64-unknown-linux-gnu`",
        ),
    ] {
        let artifact = project.join(name);
        let result = run_cli(&project, &async_entry, &artifact, &arguments);
        assert!(!result.status.success());
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(stderr.contains(expected), "{stderr}");
        assert!(!artifact.exists());
    }

    // `auto` resolves to Compio only when async root execution is reachable.
    let auto_artifact = project.join("auto-compio");
    let auto = run_cli(&project, &async_entry, &auto_artifact, &["--async", "auto"]);
    assert!(
        auto.status.success(),
        "{}",
        String::from_utf8_lossy(&auto.stderr)
    );
    let auto_stderr = String::from_utf8_lossy(&auto.stderr);
    assert!(
        auto_stderr.contains("async   compio · experimental"),
        "{auto_stderr}"
    );
    assert_execution(&auto_artifact, "cli-async 42\ncli-return 42\n");

    // Selecting Compio for synchronous source must not advertise/link async.
    let sync_artifact = project.join("sync-control");
    let sync = run_cli(
        &project,
        &sync_entry,
        &sync_artifact,
        &["--async", "compio"],
    );
    assert!(
        sync.status.success(),
        "{}",
        String::from_utf8_lossy(&sync.stderr)
    );
    let sync_stderr = String::from_utf8_lossy(&sync.stderr);
    assert!(
        !sync_stderr.contains("async   compio"),
        "synchronous CLI build advertised async backend:\n{sync_stderr}"
    );
    assert!(sync_artifact.is_file());
    assert_execution(&sync_artifact, "cli-sync 42\ncli-return 42\n");

    fs::remove_dir_all(project).unwrap();
}
