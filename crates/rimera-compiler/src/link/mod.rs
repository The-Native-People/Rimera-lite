use std::fs;
use std::path::Path;
use std::process::Command;

use crate::core::BuildProfile;

pub fn link(
    object: &Path,
    runtime_archive: &Path,
    output: &Path,
    profile: BuildProfile,
) -> Result<(), String> {
    link_objects(&[object], runtime_archive, output, profile)
}

pub fn link_objects(
    objects: &[&Path],
    runtime_archive: &Path,
    output: &Path,
    profile: BuildProfile,
) -> Result<(), String> {
    if objects.is_empty()
        || objects
            .iter()
            .any(|object| object.extension().and_then(|value| value.to_str()) != Some("o"))
    {
        return Err("native linker accepts one or more object-file compiler outputs".to_owned());
    }
    if runtime_archive.extension().and_then(|value| value.to_str()) != Some("a") {
        return Err("native runtime must be supplied as a static archive".to_owned());
    }
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut command = Command::new("clang");
    command
        .args(objects)
        .arg(runtime_archive)
        .arg("-Wl,-dead_strip");
    if profile == BuildProfile::Release {
        command.arg("-Wl,-x");
    }
    let result = command
        .arg("-o")
        .arg(output)
        .output()
        .map_err(|error| format!("failed to invoke the macOS linker driver: {error}"))?;
    if !result.status.success() {
        let _ = fs::remove_file(output);
        return Err(format!(
            "native linker failed:\n{}",
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    if profile == BuildProfile::Release {
        let result = Command::new("strip")
            .arg(output)
            .output()
            .map_err(|error| format!("failed to strip native release artifact: {error}"))?;
        if !result.status.success() {
            let _ = fs::remove_file(output);
            return Err(format!(
                "failed to strip native release artifact:\n{}",
                String::from_utf8_lossy(&result.stderr)
            ));
        }
    }
    Ok(())
}
