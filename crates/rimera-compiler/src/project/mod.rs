use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::core::{BuildProfile, TargetTriple};

/// Build/runtime capability that admits Gate 11 dynamic compilation.
///
/// Static builds intentionally leave this absent, preserving the pre-Gate-11
/// no-dynamic-compiler artifact boundary.
pub const CAPABILITY_DYNAMIC_COMPILATION: &str = "dynamic_compilation";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AsyncBackend {
    #[default]
    Auto,
    Compio,
    Monoio,
    Tokio,
}

impl AsyncBackend {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Compio => "compio",
            Self::Monoio => "monoio",
            Self::Tokio => "tokio",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapabilitySet(BTreeSet<String>);

impl CapabilitySet {
    #[must_use]
    pub fn contains(&self, capability: &str) -> bool {
        self.0.contains(capability)
    }

    #[must_use]
    pub fn dynamic_compilation(&self) -> bool {
        self.contains(CAPABILITY_DYNAMIC_COMPILATION)
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }

    #[must_use]
    pub fn from_names(names: impl IntoIterator<Item = String>) -> Self {
        Self(names.into_iter().collect())
    }
}

#[derive(Debug, Clone)]
pub struct BuildRequest {
    pub project_root: PathBuf,
    /// Ordered, explicitly declared roots used to resolve importable modules.
    /// An empty list means the project root itself.
    pub module_search_roots: Vec<PathBuf>,
    pub entry: PathBuf,
    pub output: PathBuf,
    pub target: TargetTriple,
    pub profile: BuildProfile,
    pub capabilities: CapabilitySet,
    pub debug: bool,
    pub heap_limit_bytes: Option<u64>,
    pub async_backend: AsyncBackend,
}

#[derive(Debug, Clone)]
pub struct BuildArtifact {
    pub executable: PathBuf,
    pub object: PathBuf,
    pub cache_dir: PathBuf,
    pub ir_dump: Option<PathBuf>,
    pub manifest: Option<PathBuf>,
    /// Concrete backend actually linked for source-level async root execution.
    /// `None` means this artifact has no executor backend reachability.
    pub async_backend: Option<AsyncBackend>,
}
