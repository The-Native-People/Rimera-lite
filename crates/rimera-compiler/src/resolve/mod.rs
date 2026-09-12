use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use crate::core::{Diagnostic, DiagnosticSet, Span};
use crate::syntax;

/// The absolute Python-visible identity of one module in the resolved graph.
/// Relative import spelling is resolved before a name reaches this type.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CanonicalModuleName(String);

impl CanonicalModuleName {
    pub fn parse(name: impl Into<String>) -> Result<Self, ModuleNameError> {
        let name = name.into();
        if name.is_empty() {
            return Err(ModuleNameError::Empty);
        }
        for component in name.split('.') {
            if component.is_empty() {
                return Err(ModuleNameError::EmptyComponent);
            }
            let mut characters = component.chars();
            let first = characters.next().expect("non-empty module component");
            if first != '_' && !first.is_alphabetic() {
                return Err(ModuleNameError::InvalidComponent(component.to_owned()));
            }
            if characters.any(|character| character != '_' && !character.is_alphanumeric()) {
                return Err(ModuleNameError::InvalidComponent(component.to_owned()));
            }
        }
        Ok(Self(name))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CanonicalModuleName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleNameError {
    Empty,
    EmptyComponent,
    InvalidComponent(String),
}

impl fmt::Display for ModuleNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("module name cannot be empty"),
            Self::EmptyComponent => {
                formatter.write_str("module name cannot contain an empty component")
            }
            Self::InvalidComponent(component) => {
                write!(formatter, "invalid module name component `{component}`")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleKind {
    Entry,
    Source,
    RegularPackage,
    NamespacePackage,
    NativeShell,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleNode {
    pub name: CanonicalModuleName,
    pub kind: ModuleKind,
    pub source: Option<PathBuf>,
    pub source_hash: Option<u64>,
    /// Ordered import locations. Namespace packages may own more than one.
    pub search_locations: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportEdge {
    pub importer: CanonicalModuleName,
    pub imported: CanonicalModuleName,
    /// Every source occurrence is retained even though the dependency edge is
    /// canonicalized to one ordered graph edge.
    pub occurrences: Vec<Span>,
}

#[derive(Debug, Clone)]
pub struct ModuleGraph {
    pub entry: PathBuf,
    pub entry_name: CanonicalModuleName,
    nodes: BTreeMap<CanonicalModuleName, ModuleNode>,
    edges: BTreeMap<(CanonicalModuleName, CanonicalModuleName), Vec<Span>>,
}

#[derive(Debug, Clone)]
pub struct ResolvedSource {
    pub source: String,
    pub syntax: syntax::Module,
}

#[derive(Debug, Clone)]
pub struct ResolvedProject {
    pub graph: ModuleGraph,
    pub sources: BTreeMap<CanonicalModuleName, ResolvedSource>,
}

impl ModuleGraph {
    pub fn nodes(&self) -> impl Iterator<Item = &ModuleNode> {
        self.nodes.values()
    }

    pub fn insert_node(&mut self, node: ModuleNode) -> Result<(), String> {
        if let Some(existing) = self.nodes.get_mut(&node.name) {
            let same_owner = existing.kind == node.kind
                && existing.source == node.source
                && existing.search_locations == node.search_locations;
            let compatible_hash = existing.source_hash.is_none()
                || node.source_hash.is_none()
                || existing.source_hash == node.source_hash;
            if !same_owner || !compatible_hash {
                return Err(format!("module `{}` has conflicting owners", node.name));
            }
            if existing.source_hash.is_none() {
                existing.source_hash = node.source_hash;
            }
            return Ok(());
        }
        self.nodes.insert(node.name.clone(), node);
        Ok(())
    }

    pub fn add_edge(
        &mut self,
        importer: CanonicalModuleName,
        imported: CanonicalModuleName,
        span: Span,
    ) -> Result<(), String> {
        if !self.nodes.contains_key(&importer) {
            return Err(format!("importer `{importer}` is not in the module graph"));
        }
        if !self.nodes.contains_key(&imported) {
            return Err(format!(
                "imported module `{imported}` is not in the module graph"
            ));
        }
        let occurrences = self.edges.entry((importer, imported)).or_default();
        if !occurrences.contains(&span) {
            occurrences.push(span);
            occurrences.sort_by_key(|span| (span.start, span.end));
        }
        Ok(())
    }

    pub fn edges(&self) -> impl Iterator<Item = ImportEdge> + '_ {
        self.edges
            .iter()
            .map(|((importer, imported), occurrences)| ImportEdge {
                importer: importer.clone(),
                imported: imported.clone(),
                occurrences: occurrences.clone(),
            })
    }
}

#[must_use]
pub fn single_file(entry: PathBuf) -> ModuleGraph {
    let entry_name = CanonicalModuleName::parse("__main__").expect("entry name is canonical");
    let mut nodes = BTreeMap::new();
    nodes.insert(
        entry_name.clone(),
        ModuleNode {
            name: entry_name.clone(),
            kind: ModuleKind::Entry,
            source: Some(entry.clone()),
            source_hash: None,
            search_locations: Vec::new(),
        },
    );
    ModuleGraph {
        entry,
        entry_name,
        nodes,
        edges: BTreeMap::new(),
    }
}

pub fn discover_project(
    project_root: &Path,
    entry: &Path,
) -> Result<ResolvedProject, DiagnosticSet> {
    discover_project_with_roots(project_root, entry, &[])
}

pub fn discover_project_with_roots(
    project_root: &Path,
    entry: &Path,
    declared_roots: &[PathBuf],
) -> Result<ResolvedProject, DiagnosticSet> {
    let root = fs::canonicalize(project_root).map_err(|cause| {
        diagnostic(
            "RIM-IMPORT-003",
            format!("failed to resolve project root: {cause}"),
            project_root,
            Span::default(),
        )
    })?;
    let entry = fs::canonicalize(entry).map_err(|cause| {
        diagnostic(
            "RIM-INPUT-001",
            format!("failed to resolve entry source: {cause}"),
            entry,
            Span::default(),
        )
    })?;
    if !entry.starts_with(&root) {
        return Err(diagnostic(
            "RIM-IMPORT-003",
            "entry source is outside the declared project root",
            &entry,
            Span::default(),
        ));
    }
    let search_roots = canonical_search_roots(&root, declared_roots)?;

    let entry_name = CanonicalModuleName::parse("__main__").expect("entry name is canonical");
    let mut graph = single_file(entry.clone());
    let mut sources = BTreeMap::new();
    let mut pending = BTreeSet::from([(entry_name.clone(), entry.clone())]);
    let mut path_owners = BTreeMap::<PathBuf, CanonicalModuleName>::new();
    path_owners.insert(entry.clone(), entry_name.clone());
    let mut case_owners = BTreeMap::<String, CanonicalModuleName>::new();
    case_owners.insert(entry_name.as_str().to_lowercase(), entry_name.clone());

    while let Some((name, path)) = pending.pop_first() {
        if sources.contains_key(&name) {
            continue;
        }
        let source = fs::read_to_string(&path).map_err(|cause| {
            diagnostic(
                "RIM-INPUT-001",
                format!("failed to read module `{name}`: {cause}"),
                &path,
                Span::default(),
            )
        })?;
        let mut parsed = syntax::parse(&path, &source)?;
        let kind = graph
            .nodes
            .get(&name)
            .map(|node| node.kind)
            .ok_or_else(|| {
                diagnostic(
                    "RIM-IMPORT-003",
                    format!("module `{name}` is missing from the resolved graph"),
                    &path,
                    Span::default(),
                )
            })?;
        normalize_relative_imports(&mut parsed.statements, &name, kind, &path)?;
        let source_hash = source_hash(source.as_bytes());
        if let Some(node) = graph.nodes.get_mut(&name) {
            node.source_hash = Some(source_hash);
        }

        let imports =
            imports_in(&parsed.statements)
                .into_iter()
                .flat_map(|(name, span, optional)| {
                    let mut prefix = String::new();
                    let last = name.split('.').count().saturating_sub(1);
                    name.split('.')
                        .enumerate()
                        .map(|(index, component)| {
                            if !prefix.is_empty() {
                                prefix.push('.');
                            }
                            prefix.push_str(component);
                            (prefix.clone(), span, optional && index == last)
                        })
                        .collect::<Vec<_>>()
                });
        for (imported_text, span, optional) in imports {
            let imported = CanonicalModuleName::parse(imported_text.clone())
                .map_err(|cause| diagnostic("RIM-IMPORT-002", cause.to_string(), &path, span))?;
            if matches!(
                imported.as_str(),
                "builtins"
                    | "importlib"
                    | "importlib.resources"
                    | "inspect"
                    | "weakref"
                    | "sys"
                    | "rimera"
                    | "rimera.async_runtime"
            ) {
                graph
                    .insert_node(ModuleNode {
                        name: imported.clone(),
                        kind: ModuleKind::NativeShell,
                        source: None,
                        source_hash: None,
                        search_locations: Vec::new(),
                    })
                    .map_err(|cause| diagnostic("RIM-IMPORT-002", cause, &path, span))?;
                graph
                    .add_edge(name.clone(), imported, span)
                    .map_err(|cause| diagnostic("RIM-IMPORT-002", cause, &path, span))?;
                continue;
            }

            let resolution = resolve_module(&search_roots, &imported)
                .map_err(|cause| diagnostic("RIM-IMPORT-002", cause, &path, span))?;
            let Some(resolution) = resolution else {
                if optional {
                    continue;
                }
                return Err(diagnostic(
                    "RIM-IMPORT-001",
                    format!("No module named '{}'", imported.as_str()),
                    &path,
                    span,
                ));
            };
            if let ModuleResolution::Namespace { locations } = resolution {
                graph
                    .insert_node(ModuleNode {
                        name: imported.clone(),
                        kind: ModuleKind::NamespacePackage,
                        source: None,
                        source_hash: None,
                        search_locations: locations,
                    })
                    .map_err(|cause| diagnostic("RIM-IMPORT-002", cause, &path, span))?;
                graph
                    .add_edge(name.clone(), imported, span)
                    .map_err(|cause| diagnostic("RIM-IMPORT-002", cause, &path, span))?;
                continue;
            }
            let ModuleResolution::Source { candidate, kind } = resolution else {
                unreachable!("namespace resolution returned above")
            };
            if let Some(owner) = path_owners.get(&candidate)
                && owner != &imported
            {
                return Err(diagnostic(
                    "RIM-IMPORT-002",
                    format!(
                        "module path `{}` is already owned by `{owner}`",
                        candidate.display()
                    ),
                    &path,
                    span,
                ));
            }
            let folded = imported.as_str().to_lowercase();
            if let Some(owner) = case_owners.get(&folded)
                && owner != &imported
            {
                return Err(diagnostic(
                    "RIM-IMPORT-003",
                    format!("module `{imported}` collides by case with `{owner}`"),
                    &path,
                    span,
                ));
            }
            path_owners.insert(candidate.clone(), imported.clone());
            case_owners.insert(folded, imported.clone());
            graph
                .insert_node(ModuleNode {
                    name: imported.clone(),
                    kind,
                    source: Some(candidate.clone()),
                    source_hash: None,
                    search_locations: if kind == ModuleKind::RegularPackage {
                        candidate
                            .parent()
                            .map(Path::to_path_buf)
                            .into_iter()
                            .collect()
                    } else {
                        Vec::new()
                    },
                })
                .map_err(|cause| diagnostic("RIM-IMPORT-002", cause, &path, span))?;
            graph
                .add_edge(name.clone(), imported.clone(), span)
                .map_err(|cause| diagnostic("RIM-IMPORT-002", cause, &path, span))?;
            if !sources.contains_key(&imported) {
                pending.insert((imported, candidate));
            }
        }
        sources.insert(
            name,
            ResolvedSource {
                source,
                syntax: parsed,
            },
        );
    }

    Ok(ResolvedProject { graph, sources })
}

#[derive(Debug)]
enum ModuleResolution {
    Source {
        candidate: PathBuf,
        kind: ModuleKind,
    },
    Namespace {
        locations: Vec<PathBuf>,
    },
}

fn canonical_search_roots(
    project_root: &Path,
    declared_roots: &[PathBuf],
) -> Result<Vec<PathBuf>, DiagnosticSet> {
    let roots = if declared_roots.is_empty() {
        vec![project_root.to_path_buf()]
    } else {
        declared_roots
            .iter()
            .map(|declared| {
                if declared.is_absolute() {
                    declared.clone()
                } else {
                    project_root.join(declared)
                }
            })
            .collect()
    };
    let mut canonical = Vec::with_capacity(roots.len());
    for candidate in roots {
        let resolved = fs::canonicalize(&candidate).map_err(|cause| {
            diagnostic(
                "RIM-IMPORT-003",
                format!(
                    "failed to resolve declared module root `{}`: {cause}",
                    candidate.display()
                ),
                project_root,
                Span::default(),
            )
        })?;
        if !resolved.starts_with(project_root) {
            return Err(diagnostic(
                "RIM-IMPORT-003",
                format!(
                    "declared module root `{}` is outside the project root",
                    candidate.display()
                ),
                project_root,
                Span::default(),
            ));
        }
        if !canonical.contains(&resolved) {
            canonical.push(resolved);
        }
    }
    Ok(canonical)
}

fn resolve_module(
    roots: &[PathBuf],
    name: &CanonicalModuleName,
) -> Result<Option<ModuleResolution>, String> {
    let relative = name
        .as_str()
        .split('.')
        .fold(PathBuf::new(), |path, component| path.join(component));
    let mut namespace_locations = Vec::new();
    for root in roots {
        let module_candidate = root.join(&relative).with_extension("py");
        let package_directory = root.join(&relative);
        let package_candidate = package_directory.join("__init__.py");
        let module_exists = module_candidate.is_file();
        let package_exists = package_candidate.is_file();
        if module_exists && package_exists {
            return Err(format!(
                "module `{name}` is ambiguous in `{}`: both `{}` and `{}` exist",
                root.display(),
                module_candidate.display(),
                package_candidate.display()
            ));
        }
        let source = match (module_exists, package_exists) {
            (true, false) => Some((module_candidate, ModuleKind::Source)),
            (false, true) => Some((package_candidate, ModuleKind::RegularPackage)),
            (false, false) => None,
            (true, true) => unreachable!("ambiguous candidates returned above"),
        };
        if let Some((candidate, kind)) = source {
            let candidate = fs::canonicalize(&candidate)
                .map_err(|cause| format!("failed to resolve module `{name}`: {cause}"))?;
            return Ok(Some(ModuleResolution::Source { candidate, kind }));
        }
        if package_directory.is_dir() {
            let location = fs::canonicalize(&package_directory).map_err(|cause| {
                format!("failed to resolve namespace package `{name}`: {cause}")
            })?;
            if !namespace_locations.contains(&location) {
                namespace_locations.push(location);
            }
        }
    }
    if namespace_locations.is_empty() {
        Ok(None)
    } else {
        Ok(Some(ModuleResolution::Namespace {
            locations: namespace_locations,
        }))
    }
}

#[must_use]
pub fn source_hash(source: &[u8]) -> u64 {
    source.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn imports_in(statements: &[syntax::Statement]) -> Vec<(String, Span, bool)> {
    let mut imports = Vec::new();
    for statement in statements {
        match &statement.kind {
            syntax::StatementKind::Import { aliases } => imports.extend(
                aliases
                    .iter()
                    .map(|alias| (alias.module.clone(), statement.span, false)),
            ),
            syntax::StatementKind::ImportFrom {
                module: Some(module),
                level: 0,
                aliases,
            } => {
                imports.push((module.clone(), statement.span, false));
                imports.extend(
                    aliases
                        .iter()
                        .filter(|alias| alias.module != "*")
                        .map(|alias| (format!("{module}.{}", alias.module), statement.span, true)),
                );
            }
            syntax::StatementKind::ImportFrom { .. } => {}
            syntax::StatementKind::FunctionDef { body, .. }
            | syntax::StatementKind::ClassDef { body, .. }
            | syntax::StatementKind::While { body, .. } => imports.extend(imports_in(body)),
            syntax::StatementKind::Try {
                body,
                handlers,
                else_body,
                finally_body,
                ..
            } => {
                imports.extend(imports_in(body));
                for handler in handlers {
                    imports.extend(imports_in(&handler.body));
                }
                imports.extend(imports_in(else_body));
                imports.extend(imports_in(finally_body));
            }
            syntax::StatementKind::With { body, .. } => imports.extend(imports_in(body)),
            syntax::StatementKind::If {
                then_body,
                else_body,
                ..
            } => {
                imports.extend(imports_in(then_body));
                imports.extend(imports_in(else_body));
            }
            syntax::StatementKind::For {
                body, else_body, ..
            } => {
                imports.extend(imports_in(body));
                imports.extend(imports_in(else_body));
            }
            syntax::StatementKind::Match { cases, .. } => {
                for case in cases {
                    imports.extend(imports_in(&case.body));
                }
            }
            syntax::StatementKind::Assign { .. }
            | syntax::StatementKind::AugAssign { .. }
            | syntax::StatementKind::Delete { .. }
            | syntax::StatementKind::AnnAssign { .. }
            | syntax::StatementKind::Assert { .. }
            | syntax::StatementKind::TypeAlias { .. }
            | syntax::StatementKind::Return { .. }
            | syntax::StatementKind::Break
            | syntax::StatementKind::Continue
            | syntax::StatementKind::Expression(_)
            | syntax::StatementKind::Global(_)
            | syntax::StatementKind::Nonlocal(_)
            | syntax::StatementKind::Raise { .. }
            | syntax::StatementKind::Print { .. } => {}
        }
    }
    imports
}

fn normalize_relative_imports(
    statements: &mut [syntax::Statement],
    owner: &CanonicalModuleName,
    kind: ModuleKind,
    path: &Path,
) -> Result<(), DiagnosticSet> {
    let package = match kind {
        ModuleKind::RegularPackage => Some(owner.as_str()),
        ModuleKind::Source => owner.as_str().rsplit_once('.').map(|(parent, _)| parent),
        ModuleKind::Entry | ModuleKind::NamespacePackage | ModuleKind::NativeShell => None,
    };
    for statement in statements {
        match &mut statement.kind {
            syntax::StatementKind::ImportFrom { module, level, .. } if *level > 0 => {
                let Some(package) = package else {
                    return Err(diagnostic(
                        "RIM-IMPORT-003",
                        "attempted relative import with no known parent package",
                        path,
                        statement.span,
                    ));
                };
                let mut components = package.split('.').collect::<Vec<_>>();
                let parents = (*level as usize).saturating_sub(1);
                if parents >= components.len() {
                    return Err(diagnostic(
                        "RIM-IMPORT-003",
                        "attempted relative import beyond top-level package",
                        path,
                        statement.span,
                    ));
                }
                components.truncate(components.len() - parents);
                let mut absolute = components.join(".");
                if let Some(suffix) = module.as_deref() {
                    absolute.push('.');
                    absolute.push_str(suffix);
                }
                *module = Some(absolute);
                *level = 0;
            }
            syntax::StatementKind::FunctionDef { body, .. }
            | syntax::StatementKind::ClassDef { body, .. }
            | syntax::StatementKind::While { body, .. }
            | syntax::StatementKind::With { body, .. } => {
                normalize_relative_imports(body, owner, kind, path)?;
            }
            syntax::StatementKind::Try {
                body,
                handlers,
                else_body,
                finally_body,
                ..
            } => {
                normalize_relative_imports(body, owner, kind, path)?;
                for handler in handlers {
                    normalize_relative_imports(&mut handler.body, owner, kind, path)?;
                }
                normalize_relative_imports(else_body, owner, kind, path)?;
                normalize_relative_imports(finally_body, owner, kind, path)?;
            }
            syntax::StatementKind::If {
                then_body,
                else_body,
                ..
            } => {
                normalize_relative_imports(then_body, owner, kind, path)?;
                normalize_relative_imports(else_body, owner, kind, path)?;
            }
            syntax::StatementKind::For {
                body, else_body, ..
            } => {
                normalize_relative_imports(body, owner, kind, path)?;
                normalize_relative_imports(else_body, owner, kind, path)?;
            }
            syntax::StatementKind::Match { cases, .. } => {
                for case in cases {
                    normalize_relative_imports(&mut case.body, owner, kind, path)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn diagnostic(
    code: &'static str,
    message: impl Into<String>,
    path: &Path,
    span: Span,
) -> DiagnosticSet {
    DiagnosticSet::one(Diagnostic::new(code, message, path, span))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_module_names_reject_empty_or_invalid_components() {
        for name in ["", ".pkg", "pkg.", "pkg..child", "pkg/child", "9pkg"] {
            assert!(
                CanonicalModuleName::parse(name).is_err(),
                "accepted `{name}`"
            );
        }
        for name in ["pkg", "pkg.child", "_private", "café.outil"] {
            assert_eq!(CanonicalModuleName::parse(name).unwrap().as_str(), name);
        }
    }

    #[test]
    fn module_graph_has_deterministic_nodes_edges_and_source_occurrences() {
        let mut graph = single_file(PathBuf::from("app.py"));
        let alpha = CanonicalModuleName::parse("alpha").unwrap();
        let zeta = CanonicalModuleName::parse("zeta").unwrap();
        for name in [zeta.clone(), alpha.clone()] {
            graph
                .insert_node(ModuleNode {
                    source: Some(PathBuf::from(format!("{name}.py"))),
                    source_hash: Some(1),
                    search_locations: Vec::new(),
                    name,
                    kind: ModuleKind::Source,
                })
                .unwrap();
        }

        graph
            .add_edge(graph.entry_name.clone(), zeta, Span::new(20, 24))
            .unwrap();
        graph
            .add_edge(graph.entry_name.clone(), alpha.clone(), Span::new(9, 14))
            .unwrap();
        graph
            .add_edge(graph.entry_name.clone(), alpha, Span::new(2, 7))
            .unwrap();

        assert_eq!(
            graph
                .nodes()
                .map(|node| node.name.as_str())
                .collect::<Vec<_>>(),
            ["__main__", "alpha", "zeta"]
        );
        let edges = graph.edges().collect::<Vec<_>>();
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0].imported.as_str(), "alpha");
        assert_eq!(edges[0].occurrences, [Span::new(2, 7), Span::new(9, 14)]);
        assert_eq!(edges[1].imported.as_str(), "zeta");
    }

    #[test]
    fn module_graph_rejects_conflicting_identity_owners() {
        let mut graph = single_file(PathBuf::from("app.py"));
        let name = CanonicalModuleName::parse("owned").unwrap();
        graph
            .insert_node(ModuleNode {
                name: name.clone(),
                kind: ModuleKind::Source,
                source: Some(PathBuf::from("owned.py")),
                source_hash: Some(1),
                search_locations: Vec::new(),
            })
            .unwrap();
        let failure = graph
            .insert_node(ModuleNode {
                name,
                kind: ModuleKind::RegularPackage,
                source: Some(PathBuf::from("owned/__init__.py")),
                source_hash: Some(2),
                search_locations: vec![PathBuf::from("owned")],
            })
            .unwrap_err();
        assert_eq!(failure, "module `owned` has conflicting owners");
    }

    #[test]
    fn project_discovery_is_deterministic_for_chains_diamonds_and_cycles() {
        let root =
            std::env::temp_dir().join(format!("rimera-resolve-{}-graph", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("app.py"), "import zeta\nimport alpha\n").unwrap();
        fs::write(root.join("alpha.py"), "import shared\n").unwrap();
        fs::write(root.join("zeta.py"), "import shared\n").unwrap();
        fs::write(root.join("shared.py"), "import alpha\n").unwrap();

        let first = discover_project(&root, &root.join("app.py")).unwrap();
        let second = discover_project(&root, &root.join("app.py")).unwrap();
        let names = first
            .graph
            .nodes()
            .map(|node| node.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["__main__", "alpha", "shared", "zeta"]);
        assert_eq!(
            first.graph.edges().collect::<Vec<_>>(),
            second.graph.edges().collect::<Vec<_>>()
        );
        assert_eq!(
            first
                .graph
                .nodes()
                .map(|node| (node.name.clone(), node.source_hash))
                .collect::<Vec<_>>(),
            second
                .graph
                .nodes()
                .map(|node| (node.name.clone(), node.source_hash))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn project_discovery_reports_missing_and_ambiguous_modules_at_import_spans() {
        let root =
            std::env::temp_dir().join(format!("rimera-resolve-{}-failures", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let entry = root.join("app.py");
        fs::write(&entry, "import absent\n").unwrap();
        let missing = discover_project(&root, &entry).unwrap_err();
        assert_eq!(missing.as_slice()[0].code, "RIM-IMPORT-001");
        assert!(missing.as_slice()[0].span.end > missing.as_slice()[0].span.start);

        fs::write(&entry, "import doubled\n").unwrap();
        fs::write(root.join("doubled.py"), "value = 1\n").unwrap();
        fs::create_dir_all(root.join("doubled")).unwrap();
        fs::write(root.join("doubled/__init__.py"), "value = 2\n").unwrap();
        let ambiguous = discover_project(&root, &entry).unwrap_err();
        assert_eq!(ambiguous.as_slice()[0].code, "RIM-IMPORT-002");
        assert!(ambiguous.as_slice()[0].span.end > ambiguous.as_slice()[0].span.start);
    }

    #[test]
    fn namespace_packages_merge_declared_roots_in_order() {
        let root = std::env::temp_dir().join(format!(
            "rimera-resolve-{}-namespace-roots",
            std::process::id()
        ));
        let left = root.join("left");
        let right = root.join("right");
        fs::create_dir_all(left.join("mesh")).unwrap();
        fs::create_dir_all(right.join("mesh")).unwrap();
        let entry = root.join("app.py");
        fs::write(&entry, "import mesh.left\nimport mesh.right\n").unwrap();
        fs::write(left.join("mesh/left.py"), "value = 'left'\n").unwrap();
        fs::write(right.join("mesh/right.py"), "value = 'right'\n").unwrap();

        let resolved = discover_project_with_roots(
            &root,
            &entry,
            &[PathBuf::from("left"), PathBuf::from("right")],
        )
        .unwrap();
        let namespace = resolved
            .graph
            .nodes()
            .find(|node| node.name.as_str() == "mesh")
            .unwrap();
        assert_eq!(namespace.kind, ModuleKind::NamespacePackage);
        assert_eq!(
            namespace.search_locations,
            [
                fs::canonicalize(left.join("mesh")).unwrap(),
                fs::canonicalize(right.join("mesh")).unwrap(),
            ]
        );
        assert_eq!(
            resolved
                .graph
                .nodes()
                .map(|node| node.name.as_str())
                .collect::<Vec<_>>(),
            ["__main__", "mesh", "mesh.left", "mesh.right"]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn regular_package_in_a_later_root_precedes_namespace_portions() {
        let root = std::env::temp_dir().join(format!(
            "rimera-resolve-{}-regular-precedence",
            std::process::id()
        ));
        let left = root.join("left");
        let right = root.join("right");
        fs::create_dir_all(left.join("mesh")).unwrap();
        fs::create_dir_all(right.join("mesh")).unwrap();
        let entry = root.join("app.py");
        fs::write(&entry, "import mesh\n").unwrap();
        fs::write(right.join("mesh/__init__.py"), "value = 'regular'\n").unwrap();

        let resolved = discover_project_with_roots(
            &root,
            &entry,
            &[PathBuf::from("left"), PathBuf::from("right")],
        )
        .unwrap();
        let package = resolved
            .graph
            .nodes()
            .find(|node| node.name.as_str() == "mesh")
            .unwrap();
        assert_eq!(package.kind, ModuleKind::RegularPackage);
        assert_eq!(
            package.source.as_deref(),
            Some(
                fs::canonicalize(right.join("mesh/__init__.py"))
                    .unwrap()
                    .as_path()
            )
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn declared_roots_cannot_escape_the_project() {
        let root = std::env::temp_dir().join(format!(
            "rimera-resolve-{}-root-boundary",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let entry = root.join("app.py");
        fs::write(&entry, "print('entry')\n").unwrap();
        let diagnostics =
            discover_project_with_roots(&root, &entry, &[PathBuf::from("..")]).unwrap_err();
        assert_eq!(diagnostics.as_slice()[0].code, "RIM-IMPORT-003");
        assert!(
            diagnostics.as_slice()[0]
                .message
                .contains("outside the project root")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn source_hash_changes_with_source_bytes() {
        assert_eq!(source_hash(b"same"), source_hash(b"same"));
        assert_ne!(source_hash(b"same"), source_hash(b"changed"));
    }
}
