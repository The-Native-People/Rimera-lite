use crate::mir::Program;

/// Target-level input retained as a separate boundary so layout and relocation
/// policy can evolve without changing MIR semantics.
#[derive(Debug, Clone)]
pub struct NativeProgram {
    pub mir: Program,
}

#[must_use]
pub fn select(program: &Program) -> NativeProgram {
    NativeProgram {
        mir: program.clone(),
    }
}
