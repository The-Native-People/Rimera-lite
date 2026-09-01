use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ModuleGraph {
    pub entry: PathBuf,
}

#[must_use]
pub fn single_file(entry: PathBuf) -> ModuleGraph {
    ModuleGraph { entry }
}
