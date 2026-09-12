use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::core::{Diagnostic, DiagnosticSet, Span, TargetTriple};
use crate::project::CapabilitySet;
use crate::resolve;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedResource {
    pub module: String,
    pub name: String,
    pub path: PathBuf,
    pub hash: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedModule {
    pub name: String,
    pub path: PathBuf,
    pub hash: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LockedInputs {
    pub lock_hash: u64,
    pub resources: Vec<LockedResource>,
    pub packages: Vec<(String, String)>,
    pub modules: Vec<LockedModule>,
}

pub fn validate(
    project_root: &Path,
    target: &TargetTriple,
    capabilities: &CapabilitySet,
) -> Result<LockedInputs, DiagnosticSet> {
    let lock_path = project_root.join("rimera.lock");
    let required = lock_required(project_root)?;
    if !lock_path.is_file() {
        return if required {
            Err(diagnostic(
                "RIM-LOCK-001",
                "pyproject.toml requires a lock, but rimera.lock is missing",
                &lock_path,
            ))
        } else {
            Ok(LockedInputs::default())
        };
    }
    let bytes = fs::read(&lock_path)
        .map_err(|cause| diagnostic("RIM-LOCK-001", cause.to_string(), &lock_path))?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| diagnostic("RIM-LOCK-001", "rimera.lock must be UTF-8 TOML", &lock_path))?;
    let value = text.parse::<toml::Value>().map_err(|cause| {
        diagnostic(
            "RIM-LOCK-001",
            format!("invalid rimera.lock: {cause}"),
            &lock_path,
        )
    })?;
    let table = value.as_table().ok_or_else(|| {
        diagnostic(
            "RIM-LOCK-001",
            "rimera.lock must be a TOML table",
            &lock_path,
        )
    })?;
    if table.get("version").and_then(toml::Value::as_integer) != Some(1) {
        return Err(diagnostic(
            "RIM-LOCK-001",
            "rimera.lock version must be 1",
            &lock_path,
        ));
    }
    if table
        .get("native_extensions")
        .and_then(toml::Value::as_array)
        .is_some_and(|entries| !entries.is_empty())
    {
        return Err(diagnostic(
            "RIM-LOCK-004",
            "native-extension lock entries are deferred to the extension ABI gate",
            &lock_path,
        ));
    }
    validate_string_array(table.get("targets"), "targets", &lock_path, |declared| {
        if declared.is_empty() || declared.iter().any(|item| item == target.as_str()) {
            Ok(())
        } else {
            Err(format!(
                "rimera.lock does not allow target {}",
                target.as_str()
            ))
        }
    })?;
    validate_string_array(
        table.get("capabilities"),
        "capabilities",
        &lock_path,
        |declared| {
            let denied = declared
                .iter()
                .filter(|item| !capabilities.contains(item))
                .cloned()
                .collect::<Vec<_>>();
            if denied.is_empty() {
                Ok(())
            } else {
                Err(format!(
                    "lock requires denied capabilities: {}",
                    denied.join(", ")
                ))
            }
        },
    )?;

    let mut inputs = LockedInputs {
        lock_hash: resolve::source_hash(&bytes),
        ..LockedInputs::default()
    };
    let mut owners = BTreeSet::new();
    for package in table
        .get("package")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
    {
        let package = package.as_table().ok_or_else(|| {
            diagnostic("RIM-LOCK-001", "package entries must be tables", &lock_path)
        })?;
        let name = required_string(package, "name", &lock_path)?;
        let version = required_string(package, "version", &lock_path)?;
        if !owners.insert(name.clone()) {
            return Err(diagnostic(
                "RIM-LOCK-001",
                format!("duplicate locked package '{name}'"),
                &lock_path,
            ));
        }
        inputs.packages.push((name, version));
        for module in package
            .get("module")
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
        {
            let module = module.as_table().ok_or_else(|| {
                diagnostic("RIM-LOCK-001", "module entries must be tables", &lock_path)
            })?;
            if module
                .get("native")
                .and_then(toml::Value::as_bool)
                .unwrap_or(false)
            {
                return Err(diagnostic(
                    "RIM-LOCK-004",
                    "native-extension module entries are not supported in Gate 9",
                    &lock_path,
                ));
            }
            let name = required_string(module, "name", &lock_path)?;
            resolve::CanonicalModuleName::parse(name.clone())
                .map_err(|cause| diagnostic("RIM-LOCK-001", cause.to_string(), &lock_path))?;
            let relative = PathBuf::from(required_string(module, "path", &lock_path)?);
            let expected = parse_hash(&required_string(module, "hash", &lock_path)?, &lock_path)?;
            let path = checked_project_path(project_root, &relative, &lock_path)?;
            let data = fs::read(&path).map_err(|cause| {
                diagnostic(
                    "RIM-LOCK-002",
                    format!(
                        "locked module {} cannot be read: {cause}",
                        relative.display()
                    ),
                    &lock_path,
                )
            })?;
            let actual = resolve::source_hash(&data);
            if actual != expected {
                return Err(diagnostic(
                    "RIM-LOCK-002",
                    format!(
                        "hash mismatch for locked module {}: expected {expected:016x}, found {actual:016x}",
                        relative.display()
                    ),
                    &lock_path,
                ));
            }
            inputs.modules.push(LockedModule {
                name,
                path,
                hash: actual,
            });
        }
        for resource in package
            .get("resource")
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
        {
            let resource = resource.as_table().ok_or_else(|| {
                diagnostic(
                    "RIM-LOCK-001",
                    "resource entries must be tables",
                    &lock_path,
                )
            })?;
            let module = required_string(resource, "module", &lock_path)?;
            let name = required_string(resource, "name", &lock_path)?;
            let relative = PathBuf::from(required_string(resource, "path", &lock_path)?);
            let expected = parse_hash(&required_string(resource, "hash", &lock_path)?, &lock_path)?;
            let path = checked_project_path(project_root, &relative, &lock_path)?;
            let data = fs::read(&path).map_err(|cause| {
                diagnostic(
                    "RIM-LOCK-002",
                    format!(
                        "locked resource {} cannot be read: {cause}",
                        relative.display()
                    ),
                    &lock_path,
                )
            })?;
            let actual = resolve::source_hash(&data);
            if actual != expected {
                return Err(diagnostic(
                    "RIM-LOCK-002",
                    format!(
                        "hash mismatch for locked resource {}: expected {expected:016x}, found {actual:016x}",
                        relative.display()
                    ),
                    &lock_path,
                ));
            }
            inputs.resources.push(LockedResource {
                module,
                name,
                path,
                hash: actual,
                bytes: data,
            });
        }
    }
    inputs.packages.sort();
    inputs
        .modules
        .sort_by(|left, right| left.name.cmp(&right.name));
    inputs
        .resources
        .sort_by(|left, right| (&left.module, &left.name).cmp(&(&right.module, &right.name)));
    Ok(inputs)
}

pub fn validate_graph(
    project_root: &Path,
    graph: &resolve::ModuleGraph,
    locked: &LockedInputs,
) -> Result<(), DiagnosticSet> {
    if locked.modules.is_empty() {
        return Ok(());
    }
    let lock_path = project_root.join("rimera.lock");
    let declared = locked
        .modules
        .iter()
        .map(|module| module.name.as_str())
        .collect::<BTreeSet<_>>();
    let top_levels = declared
        .iter()
        .filter_map(|name| name.split('.').next())
        .collect::<BTreeSet<_>>();
    for node in graph.nodes() {
        let name = node.name.as_str();
        let top = name.split('.').next().unwrap_or(name);
        if top_levels.contains(top)
            && !declared.contains(name)
            && !matches!(node.kind, resolve::ModuleKind::NamespacePackage)
        {
            return Err(diagnostic(
                "RIM-LOCK-003",
                format!("module '{name}' is reachable but not declared in rimera.lock"),
                &lock_path,
            ));
        }
    }
    for resource in &locked.resources {
        if !declared.contains(resource.module.as_str()) {
            return Err(diagnostic(
                "RIM-LOCK-003",
                format!(
                    "resource '{}' names undeclared module '{}'",
                    resource.name, resource.module
                ),
                &lock_path,
            ));
        }
    }
    Ok(())
}

fn lock_required(project_root: &Path) -> Result<bool, DiagnosticSet> {
    let path = project_root.join("pyproject.toml");
    if !path.is_file() {
        return Ok(false);
    }
    let text = fs::read_to_string(&path)
        .map_err(|cause| diagnostic("RIM-LOCK-001", cause.to_string(), &path))?;
    let value = text
        .parse::<toml::Value>()
        .map_err(|cause| diagnostic("RIM-LOCK-001", cause.to_string(), &path))?;
    Ok(value
        .get("tool")
        .and_then(|value| value.get("rimera"))
        .and_then(|value| value.get("locked"))
        .and_then(toml::Value::as_bool)
        .unwrap_or(false))
}

fn checked_project_path(
    project_root: &Path,
    relative: &Path,
    diagnostic_path: &Path,
) -> Result<PathBuf, DiagnosticSet> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(diagnostic(
            "RIM-LOCK-003",
            format!("locked path {} escapes the project", relative.display()),
            diagnostic_path,
        ));
    }
    Ok(project_root.join(relative))
}

fn required_string(
    table: &toml::map::Map<String, toml::Value>,
    name: &str,
    path: &Path,
) -> Result<String, DiagnosticSet> {
    table
        .get(name)
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            diagnostic(
                "RIM-LOCK-001",
                format!("missing string field '{name}'"),
                path,
            )
        })
}

fn parse_hash(value: &str, path: &Path) -> Result<u64, DiagnosticSet> {
    u64::from_str_radix(value, 16).map_err(|_| {
        diagnostic(
            "RIM-LOCK-001",
            format!("hash '{value}' must contain 16 hexadecimal digits"),
            path,
        )
    })
}

fn validate_string_array(
    value: Option<&toml::Value>,
    name: &str,
    path: &Path,
    validate: impl FnOnce(&[String]) -> Result<(), String>,
) -> Result<(), DiagnosticSet> {
    let Some(value) = value else {
        return Ok(());
    };
    let values = value
        .as_array()
        .ok_or_else(|| diagnostic("RIM-LOCK-001", format!("'{name}' must be an array"), path))?;
    let values = values
        .iter()
        .map(|value| {
            value.as_str().map(str::to_owned).ok_or_else(|| {
                diagnostic(
                    "RIM-LOCK-001",
                    format!("'{name}' entries must be strings"),
                    path,
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate(&values).map_err(|message| diagnostic("RIM-LOCK-005", message, path))
}

fn diagnostic(code: &'static str, message: impl Into<String>, path: &Path) -> DiagnosticSet {
    DiagnosticSet::one(Diagnostic::new(code, message, path, Span::new(0, 1)))
}

pub fn render_build_manifest(
    target: &TargetTriple,
    profile: crate::core::BuildProfile,
    capabilities: &CapabilitySet,
    graph: &resolve::ModuleGraph,
    locked: &LockedInputs,
) -> String {
    let mut output = format!(
        "version = 1\nruntime_abi = {}\ntarget = {:?}\nprofile = {:?}\nlock_hash = \"{:016x}\"\n",
        rimera_abi::ABI_VERSION,
        target.as_str(),
        format!("{profile:?}").to_lowercase(),
        locked.lock_hash
    );
    let capability_values = capabilities
        .iter()
        .map(|value| format!("{value:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    output.push_str(&format!("capabilities = [{capability_values}]\n"));
    for node in graph.nodes() {
        output.push_str("\n[[module]]\n");
        output.push_str(&format!(
            "name = {:?}\nkind = {:?}\n",
            node.name.as_str(),
            format!("{:?}", node.kind)
        ));
        if let Some(hash) = node.source_hash {
            output.push_str(&format!("source_hash = \"{hash:016x}\"\n"));
        }
    }
    for (name, version) in &locked.packages {
        output.push_str(&format!(
            "\n[[package]]\nname = {name:?}\nversion = {version:?}\n"
        ));
    }
    for module in &locked.modules {
        output.push_str(&format!(
            "\n[[locked_module]]\nname = {:?}\npath = {:?}\nhash = \"{:016x}\"\n",
            module.name,
            module.path.display().to_string(),
            module.hash
        ));
    }
    for resource in &locked.resources {
        output.push_str(&format!(
            "\n[[resource]]\nmodule = {:?}\nname = {:?}\npath = {:?}\nhash = \"{:016x}\"\n",
            resource.module,
            resource.name,
            resource.path.display().to_string(),
            resource.hash
        ));
    }
    output
}
