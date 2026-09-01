use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::core::{BuildProfile, TargetTriple};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapabilitySet(BTreeSet<String>);

impl CapabilitySet {
    #[must_use]
    pub fn contains(&self, capability: &str) -> bool {
        self.0.contains(capability)
    }
}

#[derive(Debug, Clone)]
pub struct BuildRequest {
    pub project_root: PathBuf,
    pub entry: PathBuf,
    pub output: PathBuf,
    pub target: TargetTriple,
    pub profile: BuildProfile,
    pub capabilities: CapabilitySet,
    pub debug: bool,
    pub heap_limit_bytes: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct BuildArtifact {
    pub executable: PathBuf,
    pub object: PathBuf,
    pub cache_dir: PathBuf,
    pub ir_dump: Option<PathBuf>,
}
