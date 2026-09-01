use std::collections::{BTreeMap, BTreeSet};

use crate::core::Span;
use rimera_abi::RParameterKind;
pub use rimera_abi::{
    RBinaryOperator as BinaryOperator, RCallArgumentKind as CallArgumentKind,
    RCompareOperator as CompareOperator, RUnaryOperator as UnaryOperator,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValueId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FunctionId(pub u32);

/// The execution role of a compiled native function.
///
/// Class bodies deliberately remain compiled functions: this keeps class
/// execution in the same verified MIR and Cranelift pipeline as Python code
/// while making the prepared namespace an explicit input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionKind {
    Module,
    Python,
    ClassBody,
}

#[derive(Debug, Clone)]
pub struct Program {
    pub filename: String,
    pub line_starts: Vec<u32>,
    pub entry: FunctionId,
    pub functions: Vec<Function>,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub kind: FunctionKind,
    pub name: String,
    pub qualified_name: String,
    pub parameters: Vec<Parameter>,
    pub entry: BlockId,
    pub blocks: Vec<Block>,
    pub value_count: u32,
    pub exception_edges: BTreeMap<(u32, u32), BlockId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    pub value: ValueId,
    pub name: String,
    pub kind: RParameterKind,
    pub has_default: bool,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub parameters: Vec<ValueId>,
    pub operations: Vec<Operation>,
    pub terminator: Terminator,
}

#[derive(Debug, Clone)]
pub struct Operation {
    pub span: Span,
    pub kind: OperationKind,
}

#[derive(Debug, Clone)]
pub enum OperationKind {
    Constant {
        dest: ValueId,
        value: Constant,
    },
    Copy {
        dest: ValueId,
        source: ValueId,
    },
    Unary {
        dest: ValueId,
        op: UnaryOperator,
        operand: ValueId,
    },
    Binary {
        dest: ValueId,
        op: BinaryOperator,
        left: ValueId,
        right: ValueId,
    },
    InPlace {
        dest: ValueId,
        op: BinaryOperator,
        left: ValueId,
        right: ValueId,
    },
    Compare {
        dest: ValueId,
        op: CompareOperator,
        left: ValueId,
        right: ValueId,
    },
    ValueArray {
        dest: ValueId,
        values: Vec<ValueId>,
    },
    Tuple {
        dest: ValueId,
        values: Vec<ValueId>,
    },
    List {
        dest: ValueId,
        values: Vec<ValueId>,
    },
    Dictionary {
        dest: ValueId,
        keys: Vec<ValueId>,
        values: Vec<ValueId>,
    },
    DictionaryMerge {
        dictionary: ValueId,
        source: ValueId,
    },
    Set {
        dest: ValueId,
        values: Vec<ValueId>,
    },
    SliceNew {
        dest: ValueId,
        start: Option<ValueId>,
        stop: Option<ValueId>,
        step: Option<ValueId>,
    },
    Contains {
        dest: ValueId,
        collection: ValueId,
        needle: ValueId,
        negate: bool,
    },
    Unpack {
        dest: ValueId,
        value: ValueId,
        before_count: u32,
        after_count: u32,
        starred: bool,
    },
    ItemGet {
        dest: ValueId,
        collection: ValueId,
        index: ValueId,
    },
    Length {
        dest: ValueId,
        value: ValueId,
    },
    IteratorNew {
        dest: ValueId,
        value: ValueId,
    },
    IteratorNext {
        item: ValueId,
        has_value: ValueId,
        iterator: ValueId,
    },
    Range {
        dest: ValueId,
        start: ValueId,
        stop: ValueId,
        step: ValueId,
    },
    ItemSet {
        collection: ValueId,
        index: ValueId,
        value: ValueId,
    },
    ItemDelete {
        collection: ValueId,
        index: ValueId,
    },
    ValueArrayGet {
        dest: ValueId,
        array: ValueId,
        index: u32,
    },
    MakeFunction {
        dest: ValueId,
        function: FunctionId,
        defaults: Vec<(u32, ValueId)>,
        closure: Vec<ValueId>,
    },
    ClassNamespaceNew {
        dest: ValueId,
    },
    ClassNamespaceSet {
        namespace: ValueId,
        name: String,
        value: ValueId,
    },
    ClassNamespaceGet {
        dest: ValueId,
        namespace: ValueId,
        name: String,
    },
    /// Resolves a name while executing a class body. The runtime checks the
    /// prepared namespace first, then ordinary global/builtin lookup.
    ClassNameGet {
        dest: ValueId,
        namespace: ValueId,
        name: String,
    },
    ClassNamespaceDelete {
        namespace: ValueId,
        name: String,
    },
    ClassNew {
        dest: ValueId,
        name: String,
        bases: Vec<ValueId>,
        namespace: ValueId,
    },
    AttributeGet {
        dest: ValueId,
        receiver: ValueId,
        name: String,
    },
    AttributeSet {
        receiver: ValueId,
        name: String,
        value: ValueId,
    },
    AttributeDelete {
        receiver: ValueId,
        name: String,
    },
    Call {
        dest: ValueId,
        callable: ValueId,
        positional: Vec<ValueId>,
        keywords: Vec<(String, ValueId)>,
    },
    CallArgumentsNew {
        dest: ValueId,
        callable: ValueId,
    },
    CallArgumentAdd {
        arguments: ValueId,
        kind: CallArgumentKind,
        name: Option<String>,
        value: ValueId,
    },
    CallPrepared {
        dest: ValueId,
        callable: ValueId,
        arguments: ValueId,
    },
    CallModuleChunk {
        function: FunctionId,
    },
    CellNew {
        dest: ValueId,
        initial: Option<ValueId>,
    },
    CellGet {
        dest: ValueId,
        cell: ValueId,
        name: Option<String>,
        free: bool,
    },
    ClosureGet {
        dest: ValueId,
        index: u32,
    },
    CellSet {
        cell: ValueId,
        value: ValueId,
    },
    CellClear {
        cell: ValueId,
    },
    GlobalGet {
        dest: ValueId,
        name: String,
    },
    GlobalSet {
        name: String,
        value: ValueId,
    },
    GlobalDelete {
        name: String,
    },
    ExceptionActive {
        dest: ValueId,
    },
    ExceptionMatches {
        dest: ValueId,
        exception: ValueId,
        expected_type: ValueId,
    },
    ExceptionSplit {
        dest: ValueId,
        exception: ValueId,
        expected_type: ValueId,
    },
    ExceptionSetActive {
        exception: ValueId,
    },
    ExceptionMerge {
        remainder: ValueId,
    },
    ExceptionCombine {
        dest: ValueId,
        left: ValueId,
        right: ValueId,
    },
    ExceptionClearActive,
    HandlerEnter {
        dest: ValueId,
    },
    HandlerLeave,
    Raise {
        exception: ValueId,
        cause: Option<ValueId>,
        suppress_context: bool,
    },
    Reraise,
    Propagate,
    Print {
        values: Vec<ValueId>,
    },
    PrintLiteral {
        value: String,
    },
    Collect,
}

impl OperationKind {
    #[must_use]
    pub fn destination(&self) -> Option<ValueId> {
        match self {
            Self::Constant { dest, .. }
            | Self::Copy { dest, .. }
            | Self::Unary { dest, .. }
            | Self::Binary { dest, .. }
            | Self::InPlace { dest, .. }
            | Self::Compare { dest, .. }
            | Self::ValueArray { dest, .. }
            | Self::Tuple { dest, .. }
            | Self::List { dest, .. }
            | Self::Dictionary { dest, .. }
            | Self::Set { dest, .. }
            | Self::SliceNew { dest, .. }
            | Self::Contains { dest, .. }
            | Self::Unpack { dest, .. }
            | Self::ItemGet { dest, .. }
            | Self::Length { dest, .. }
            | Self::IteratorNew { dest, .. }
            | Self::Range { dest, .. }
            | Self::ValueArrayGet { dest, .. }
            | Self::MakeFunction { dest, .. }
            | Self::ClassNamespaceNew { dest }
            | Self::ClassNamespaceGet { dest, .. }
            | Self::ClassNameGet { dest, .. }
            | Self::ClassNew { dest, .. }
            | Self::AttributeGet { dest, .. }
            | Self::Call { dest, .. }
            | Self::CallArgumentsNew { dest, .. }
            | Self::CallPrepared { dest, .. }
            | Self::CellNew { dest, .. }
            | Self::CellGet { dest, .. }
            | Self::ClosureGet { dest, .. }
            | Self::GlobalGet { dest, .. } => Some(*dest),
            Self::ExceptionActive { dest }
            | Self::ExceptionMatches { dest, .. }
            | Self::ExceptionSplit { dest, .. }
            | Self::ExceptionCombine { dest, .. }
            | Self::HandlerEnter { dest } => Some(*dest),
            Self::CellSet { .. }
            | Self::CellClear { .. }
            | Self::GlobalSet { .. }
            | Self::GlobalDelete { .. }
            | Self::ClassNamespaceSet { .. }
            | Self::ClassNamespaceDelete { .. }
            | Self::AttributeSet { .. }
            | Self::AttributeDelete { .. }
            | Self::HandlerLeave
            | Self::ExceptionSetActive { .. }
            | Self::ExceptionMerge { .. }
            | Self::ExceptionClearActive
            | Self::ItemSet { .. }
            | Self::ItemDelete { .. }
            | Self::DictionaryMerge { .. }
            | Self::CallArgumentAdd { .. }
            | Self::IteratorNext { .. }
            | Self::CallModuleChunk { .. }
            | Self::Raise { .. }
            | Self::Reraise
            | Self::Propagate
            | Self::Print { .. }
            | Self::PrintLiteral { .. }
            | Self::Collect => None,
        }
    }

    fn destinations(&self) -> Vec<ValueId> {
        match self {
            Self::IteratorNext {
                item, has_value, ..
            } => vec![*item, *has_value],
            _ => self.destination().into_iter().collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Constant {
    None,
    Bool(bool),
    Int(String),
    Float(u64),
    String(String),
    Bytes(Vec<u8>),
    Complex { real: u64, imag: u64 },
}

#[derive(Debug, Clone)]
pub enum Terminator {
    Jump {
        target: BlockId,
        arguments: Vec<ValueId>,
    },
    Branch {
        condition: ValueId,
        then_target: BlockId,
        then_arguments: Vec<ValueId>,
        else_target: BlockId,
        else_arguments: Vec<ValueId>,
    },
    Return {
        code: i32,
    },
    ReturnValue {
        value: Option<ValueId>,
    },
    Unreachable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafepointPlan {
    operation_roots: BTreeMap<(usize, usize), Vec<ValueId>>,
    terminator_roots: BTreeMap<usize, Vec<ValueId>>,
    max_roots: usize,
}

impl SafepointPlan {
    #[must_use]
    pub fn operation_roots(&self, block: usize, operation: usize) -> Option<&[ValueId]> {
        self.operation_roots
            .get(&(block, operation))
            .map(Vec::as_slice)
    }

    #[must_use]
    pub fn terminator_roots(&self, block: usize) -> Option<&[ValueId]> {
        self.terminator_roots.get(&block).map(Vec::as_slice)
    }

    #[must_use]
    pub const fn max_roots(&self) -> usize {
        self.max_roots
    }
}

pub fn safepoint_plan(program: &Function) -> Result<SafepointPlan, String> {
    verify_function(program)?;
    let mut live_in = vec![BTreeSet::new(); program.blocks.len()];
    loop {
        let mut changed = false;
        for (block_index, block) in program.blocks.iter().enumerate().rev() {
            let mut live = successor_live_values(program, block, &live_in);
            live.extend(terminator_inputs(&block.terminator));
            for operation in block.operations.iter().rev() {
                for destination in operation.kind.destinations() {
                    live.remove(&destination);
                }
                live.extend(operation_inputs(&operation.kind));
            }
            for parameter in &block.parameters {
                live.remove(parameter);
            }
            if live != live_in[block_index] {
                live_in[block_index] = live;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut operation_roots = BTreeMap::new();
    let mut terminator_roots = BTreeMap::new();
    let mut max_roots = 0;
    for (block_index, block) in program.blocks.iter().enumerate() {
        let mut live = successor_live_values(program, block, &live_in);
        live.extend(terminator_inputs(&block.terminator));
        if matches!(block.terminator, Terminator::Branch { .. }) {
            let roots = live.iter().copied().collect::<Vec<_>>();
            max_roots = max_roots.max(roots.len());
            terminator_roots.insert(block_index, roots);
        }
        for (operation_index, operation) in block.operations.iter().enumerate().rev() {
            for destination in operation.kind.destinations() {
                live.remove(&destination);
            }
            live.extend(operation_inputs(&operation.kind));
            if operation_is_safepoint(&operation.kind) {
                let roots = live.iter().copied().collect::<Vec<_>>();
                max_roots = max_roots.max(roots.len());
                operation_roots.insert((block_index, operation_index), roots);
            }
        }
    }
    let plan = SafepointPlan {
        operation_roots,
        terminator_roots,
        max_roots,
    };
    verify_safepoint_plan(program, &plan)?;
    Ok(plan)
}

fn verify_safepoint_plan(program: &Function, plan: &SafepointPlan) -> Result<(), String> {
    let mut observed_max = 0;
    for (block_index, block) in program.blocks.iter().enumerate() {
        for (operation_index, operation) in block.operations.iter().enumerate() {
            let roots = plan.operation_roots(block_index, operation_index);
            if operation_is_safepoint(&operation.kind) != roots.is_some() {
                return Err(format!(
                    "MIR safepoint roots are inconsistent at block {block_index} operation {operation_index}"
                ));
            }
            if let Some(roots) = roots {
                for value in roots {
                    verify_value(*value, program.value_count)?;
                }
                observed_max = observed_max.max(roots.len());
            }
        }
        let roots = plan.terminator_roots(block_index);
        if matches!(block.terminator, Terminator::Branch { .. }) != roots.is_some() {
            return Err(format!(
                "MIR terminator roots are inconsistent at block {block_index}"
            ));
        }
        if let Some(roots) = roots {
            for value in roots {
                verify_value(*value, program.value_count)?;
            }
            observed_max = observed_max.max(roots.len());
        }
    }
    if observed_max != plan.max_roots {
        return Err("MIR safepoint root capacity is inconsistent".to_owned());
    }
    Ok(())
}

fn successor_live_values(
    program: &Function,
    block: &Block,
    live_in: &[BTreeSet<ValueId>],
) -> BTreeSet<ValueId> {
    terminator_successors(&block.terminator)
        .into_iter()
        .flat_map(|target| live_in[target.0 as usize].iter().copied())
        .filter(|value| value.0 < program.value_count)
        .collect()
}

fn operation_is_safepoint(operation: &OperationKind) -> bool {
    match operation {
        OperationKind::Constant {
            value: Constant::Int(value),
            ..
        } => value.parse::<i64>().is_err(),
        OperationKind::Constant {
            value: Constant::String(_) | Constant::Bytes(_) | Constant::Complex { .. },
            ..
        }
        | OperationKind::Unary { .. }
        | OperationKind::Binary { .. }
        | OperationKind::InPlace { .. }
        | OperationKind::Compare { .. }
        | OperationKind::ValueArray { .. }
        | OperationKind::Tuple { .. }
        | OperationKind::List { .. }
        | OperationKind::Dictionary { .. }
        | OperationKind::DictionaryMerge { .. }
        | OperationKind::Set { .. }
        | OperationKind::SliceNew { .. }
        | OperationKind::Contains { .. }
        | OperationKind::Unpack { .. }
        | OperationKind::ItemGet { .. }
        | OperationKind::Length { .. }
        | OperationKind::IteratorNew { .. }
        | OperationKind::IteratorNext { .. }
        | OperationKind::Range { .. }
        | OperationKind::ItemSet { .. }
        | OperationKind::ItemDelete { .. }
        | OperationKind::ValueArrayGet { .. }
        | OperationKind::MakeFunction { .. }
        | OperationKind::ClassNamespaceNew { .. }
        | OperationKind::ClassNamespaceGet { .. }
        | OperationKind::ClassNameGet { .. }
        | OperationKind::ClassNamespaceSet { .. }
        | OperationKind::ClassNamespaceDelete { .. }
        | OperationKind::ClassNew { .. }
        | OperationKind::AttributeGet { .. }
        | OperationKind::AttributeSet { .. }
        | OperationKind::AttributeDelete { .. }
        | OperationKind::Call { .. }
        | OperationKind::CallArgumentsNew { .. }
        | OperationKind::CallArgumentAdd { .. }
        | OperationKind::CallPrepared { .. }
        | OperationKind::CallModuleChunk { .. }
        | OperationKind::CellNew { .. }
        | OperationKind::CellGet { .. }
        | OperationKind::ClosureGet { .. }
        | OperationKind::CellSet { .. }
        | OperationKind::CellClear { .. }
        | OperationKind::GlobalGet { .. }
        | OperationKind::GlobalSet { .. }
        | OperationKind::GlobalDelete { .. }
        | OperationKind::ExceptionActive { .. }
        | OperationKind::ExceptionMatches { .. }
        | OperationKind::ExceptionSplit { .. }
        | OperationKind::ExceptionSetActive { .. }
        | OperationKind::ExceptionMerge { .. }
        | OperationKind::ExceptionCombine { .. }
        | OperationKind::ExceptionClearActive
        | OperationKind::HandlerEnter { .. }
        | OperationKind::HandlerLeave
        | OperationKind::Raise { .. }
        | OperationKind::Reraise
        | OperationKind::Propagate
        | OperationKind::Print { .. }
        | OperationKind::PrintLiteral { .. }
        | OperationKind::Collect => true,
        OperationKind::Constant { .. } | OperationKind::Copy { .. } => false,
    }
}

fn terminator_inputs(terminator: &Terminator) -> Vec<ValueId> {
    match terminator {
        Terminator::Jump { arguments, .. } => arguments.clone(),
        Terminator::Branch {
            condition,
            then_arguments,
            else_arguments,
            ..
        } => std::iter::once(*condition)
            .chain(then_arguments.iter().copied())
            .chain(else_arguments.iter().copied())
            .collect(),
        Terminator::Return { .. } | Terminator::Unreachable => Vec::new(),
        Terminator::ReturnValue { value } => value.iter().copied().collect(),
    }
}

fn terminator_successors(terminator: &Terminator) -> Vec<BlockId> {
    match terminator {
        Terminator::Jump { target, .. } => vec![*target],
        Terminator::Branch {
            then_target,
            else_target,
            ..
        } => vec![*then_target, *else_target],
        Terminator::Return { .. } | Terminator::ReturnValue { .. } | Terminator::Unreachable => {
            Vec::new()
        }
    }
}

pub fn verify(program: &Program) -> Result<(), String> {
    if program.functions.is_empty() {
        return Err("MIR program has no functions".to_owned());
    }
    if program.entry.0 as usize >= program.functions.len() {
        return Err("MIR entry function does not exist".to_owned());
    }
    for function in &program.functions {
        verify_function(function)?;
        for ((block, operation), target) in &function.exception_edges {
            let Some(block_value) = function.blocks.get(*block as usize) else {
                return Err("MIR exception edge references a missing source block".to_owned());
            };
            if block_value.operations.get(*operation as usize).is_none() {
                return Err("MIR exception edge references a missing operation".to_owned());
            }
            if function.blocks.get(target.0 as usize).is_none() {
                return Err("MIR exception edge references a missing target block".to_owned());
            }
        }
        for block in &function.blocks {
            for operation in &block.operations {
                if let OperationKind::MakeFunction {
                    function: referenced,
                    defaults,
                    ..
                } = &operation.kind
                {
                    let Some(target) = program.functions.get(referenced.0 as usize) else {
                        return Err(format!("MIR references missing function {}", referenced.0));
                    };
                    for (parameter, _) in defaults {
                        let Some(parameter) = target.parameters.get(*parameter as usize) else {
                            return Err(
                                "MIR function default references a missing parameter".to_owned()
                            );
                        };
                        if !parameter.has_default {
                            return Err(
                                "MIR supplies a default for a required parameter".to_owned()
                            );
                        }
                    }
                }
                if let OperationKind::CallModuleChunk { function } = operation.kind {
                    let Some(target) = program.functions.get(function.0 as usize) else {
                        return Err(format!(
                            "MIR references missing module chunk {}",
                            function.0
                        ));
                    };
                    if !target.parameters.is_empty() {
                        return Err("MIR module chunks cannot declare parameters".to_owned());
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn verify_function(program: &Function) -> Result<(), String> {
    if program.blocks.is_empty() {
        return Err("MIR program has no blocks".to_owned());
    }
    if program.entry.0 as usize >= program.blocks.len() {
        return Err("MIR entry block does not exist".to_owned());
    }
    let mut definitions = program
        .parameters
        .iter()
        .map(|parameter| parameter.value)
        .collect::<BTreeSet<_>>();
    if definitions.len() != program.parameters.len() {
        return Err("MIR function parameters contain duplicate values".to_owned());
    }
    let mut parameter_names = BTreeSet::new();
    for parameter in &program.parameters {
        verify_value(parameter.value, program.value_count)?;
        if !parameter_names.insert(parameter.name.as_str()) {
            return Err(format!(
                "MIR function parameter `{}` is defined more than once",
                parameter.name
            ));
        }
    }
    for (block_index, block) in program.blocks.iter().enumerate() {
        for parameter in &block.parameters {
            verify_value(*parameter, program.value_count)?;
            if !definitions.insert(*parameter) {
                return Err(format!(
                    "MIR value %{} is defined more than once",
                    parameter.0
                ));
            }
        }
        for operation in &block.operations {
            for destination in operation.kind.destinations() {
                verify_value(destination, program.value_count)?;
                if !definitions.insert(destination) {
                    return Err(format!(
                        "MIR value %{} is defined more than once",
                        destination.0
                    ));
                }
            }
            for input in operation_inputs(&operation.kind) {
                verify_value(input, program.value_count)?;
            }
        }
        verify_terminator(program, block_index, &block.terminator)?;
    }
    if definitions.len() != program.value_count as usize {
        return Err("MIR contains allocated values without definitions".to_owned());
    }
    Ok(())
}

#[must_use]
pub fn render_python(program: &Program) -> String {
    use std::fmt::Write;

    let mut output = String::from("# IR REPRESENTATION\n");
    for (function_index, function) in program.functions.iter().enumerate() {
        let _ = writeln!(
            output,
            "# function {}: {}",
            function_index, function.qualified_name
        );
        render_function(&mut output, function);
    }
    output
}

fn render_function(output: &mut String, program: &Function) {
    use std::fmt::Write;

    for (block_index, block) in program.blocks.iter().enumerate() {
        let _ = writeln!(output, "# block {block_index}");
        if !block.parameters.is_empty() {
            let parameters = block
                .parameters
                .iter()
                .map(|value| format!("v{}", value.0))
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(output, "# parameters: {parameters}");
        }
        for operation in &block.operations {
            let _ = writeln!(output, "{}", render_operation(operation));
        }
        let _ = writeln!(output, "{}", render_terminator(&block.terminator));
        output.push('\n');
    }
}

fn render_operation(operation: &Operation) -> String {
    match &operation.kind {
        OperationKind::Constant { dest, value } => match value {
            Constant::None => format!("v{} = None", dest.0),
            Constant::Bool(value) => format!("v{} = {value}", dest.0),
            Constant::Int(value) => format!("v{} = {value}", dest.0),
            Constant::Float(bits) => format!("v{} = {}", dest.0, f64::from_bits(*bits)),
            Constant::String(value) => format!("v{} = {value:?}", dest.0),
            Constant::Bytes(value) => format!("v{} = b{value:?}", dest.0),
            Constant::Complex { real, imag } => format!(
                "v{} = {}+{}j",
                dest.0,
                f64::from_bits(*real),
                f64::from_bits(*imag)
            ),
        },
        OperationKind::Copy { dest, source } => format!("v{} = v{}", dest.0, source.0),
        OperationKind::Unary { dest, op, operand } => format!(
            "v{} = {}v{}",
            dest.0,
            match op {
                UnaryOperator::Positive => "+",
                UnaryOperator::Negate => "-",
                UnaryOperator::Invert => "~",
                UnaryOperator::Not => "not ",
            },
            operand.0
        ),
        OperationKind::Binary {
            dest,
            op,
            left,
            right,
        } => format!(
            "v{} = v{} {} v{}",
            dest.0,
            left.0,
            match op {
                BinaryOperator::Add => "+",
                BinaryOperator::Subtract => "-",
                BinaryOperator::Multiply => "*",
                BinaryOperator::FloorDivide => "//",
                BinaryOperator::Modulo => "%",
                BinaryOperator::TrueDivide => "/",
                BinaryOperator::Power => "**",
                BinaryOperator::LeftShift => "<<",
                BinaryOperator::RightShift => ">>",
                BinaryOperator::BitAnd => "&",
                BinaryOperator::BitXor => "^",
                BinaryOperator::BitOr => "|",
                BinaryOperator::MatrixMultiply => "@",
            },
            right.0
        ),
        OperationKind::InPlace {
            dest,
            op,
            left,
            right,
        } => format!("v{} = inplace_{op:?}(v{}, v{})", dest.0, left.0, right.0),
        OperationKind::Compare {
            dest,
            op,
            left,
            right,
        } => format!(
            "v{} = v{} {} v{}",
            dest.0,
            left.0,
            match op {
                CompareOperator::Equal => "==",
                CompareOperator::NotEqual => "!=",
                CompareOperator::Less => "<",
                CompareOperator::LessEqual => "<=",
                CompareOperator::Greater => ">",
                CompareOperator::GreaterEqual => ">=",
                CompareOperator::In => "in",
                CompareOperator::NotIn => "not in",
                CompareOperator::Is => "is",
                CompareOperator::IsNot => "is not",
            },
            right.0
        ),
        OperationKind::ValueArray { dest, values } => format!(
            "v{} = value_array({})",
            dest.0,
            values
                .iter()
                .map(|value| format!("v{}", value.0))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        OperationKind::Tuple { dest, values } => format!(
            "v{} = tuple({})",
            dest.0,
            values
                .iter()
                .map(|value| format!("v{}", value.0))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        OperationKind::List { dest, values } => format!(
            "v{} = list({})",
            dest.0,
            values
                .iter()
                .map(|value| format!("v{}", value.0))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        OperationKind::Dictionary { dest, keys, values } => format!(
            "v{} = dict({})",
            dest.0,
            keys.iter()
                .zip(values)
                .map(|(key, value)| format!("v{}: v{}", key.0, value.0))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        OperationKind::Set { dest, values } => format!(
            "v{} = set({})",
            dest.0,
            values
                .iter()
                .map(|value| format!("v{}", value.0))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        OperationKind::SliceNew {
            dest,
            start,
            stop,
            step,
        } => format!(
            "v{} = slice({:?}, {:?}, {:?})",
            dest.0,
            start.map(|value| value.0),
            stop.map(|value| value.0),
            step.map(|value| value.0)
        ),
        OperationKind::Contains {
            dest,
            collection,
            needle,
            negate,
        } => format!(
            "v{} = {}contains(v{}, v{})",
            dest.0,
            if *negate { "not " } else { "" },
            collection.0,
            needle.0
        ),
        OperationKind::Unpack {
            dest,
            value,
            before_count,
            after_count,
            starred,
        } => format!(
            "v{} = unpack(v{}, before={}, after={}, starred={})",
            dest.0, value.0, before_count, after_count, starred
        ),
        OperationKind::ItemGet {
            dest,
            collection,
            index,
        } => format!("v{} = item_get(v{}, v{})", dest.0, collection.0, index.0),
        OperationKind::Length { dest, value } => format!("v{} = len(v{})", dest.0, value.0),
        OperationKind::IteratorNew { dest, value } => format!("v{} = iter(v{})", dest.0, value.0),
        OperationKind::IteratorNext {
            item,
            has_value,
            iterator,
        } => format!("v{}, v{} = next(v{})", item.0, has_value.0, iterator.0),
        OperationKind::Range {
            dest,
            start,
            stop,
            step,
        } => format!(
            "v{} = range(v{}, v{}, v{})",
            dest.0, start.0, stop.0, step.0
        ),
        OperationKind::ItemSet {
            collection,
            index,
            value,
        } => format!("item_set(v{}, v{}, v{})", collection.0, index.0, value.0),
        OperationKind::ItemDelete { collection, index } => {
            format!("item_delete(v{}, v{})", collection.0, index.0)
        }
        OperationKind::ValueArrayGet { dest, array, index } => {
            format!("v{} = value_array_get(v{}, {index})", dest.0, array.0)
        }
        OperationKind::MakeFunction {
            dest,
            function,
            defaults,
            closure,
        } => format!(
            "v{} = make_function({}, defaults={:?}, closure={:?})",
            dest.0, function.0, defaults, closure
        ),
        OperationKind::ClassNamespaceNew { dest } => format!("v{} = class_namespace_new()", dest.0),
        OperationKind::ClassNamespaceSet {
            namespace,
            name,
            value,
        } => format!(
            "class_namespace_set(v{}, {name:?}, v{})",
            namespace.0, value.0
        ),
        OperationKind::ClassNamespaceGet {
            dest,
            namespace,
            name,
        } => format!(
            "v{} = class_namespace_get(v{}, {name:?})",
            dest.0, namespace.0
        ),
        OperationKind::ClassNameGet {
            dest,
            namespace,
            name,
        } => format!("v{} = class_name_get(v{}, {name:?})", dest.0, namespace.0),
        OperationKind::ClassNamespaceDelete { namespace, name } => {
            format!("class_namespace_delete(v{}, {name:?})", namespace.0)
        }
        OperationKind::ClassNew {
            dest,
            name,
            bases,
            namespace,
        } => format!(
            "v{} = class_new({name:?}, bases={bases:?}, v{})",
            dest.0, namespace.0
        ),
        OperationKind::AttributeGet {
            dest,
            receiver,
            name,
        } => format!("v{} = attr_get(v{}, {name:?})", dest.0, receiver.0),
        OperationKind::AttributeSet {
            receiver,
            name,
            value,
        } => format!("attr_set(v{}, {name:?}, v{})", receiver.0, value.0),
        OperationKind::AttributeDelete { receiver, name } => {
            format!("attr_delete(v{}, {name:?})", receiver.0)
        }
        OperationKind::Call {
            dest,
            callable,
            positional,
            keywords,
        } => format!(
            "v{} = call(v{}, positional={:?}, keywords={:?})",
            dest.0, callable.0, positional, keywords
        ),
        OperationKind::CallArgumentsNew { dest, callable } => {
            format!("v{} = call_arguments_new(v{})", dest.0, callable.0)
        }
        OperationKind::CallArgumentAdd {
            arguments,
            kind,
            name,
            value,
        } => format!(
            "call_argument_add(v{}, {kind:?}, {name:?}, v{})",
            arguments.0, value.0
        ),
        OperationKind::CallPrepared {
            dest,
            callable,
            arguments,
        } => format!(
            "v{} = call_prepared(v{}, v{})",
            dest.0, callable.0, arguments.0
        ),
        OperationKind::CallModuleChunk { function } => {
            format!("call_module_chunk({})", function.0)
        }
        OperationKind::CellNew { dest, initial } => {
            format!("v{} = cell_new({initial:?})", dest.0)
        }
        OperationKind::CellGet {
            dest,
            cell,
            name,
            free,
        } => format!("v{} = cell_get(v{}, {name:?}, free={free})", dest.0, cell.0),
        OperationKind::ClosureGet { dest, index } => {
            format!("v{} = closure_get({index})", dest.0)
        }
        OperationKind::CellSet { cell, value } => {
            format!("cell_set(v{}, v{})", cell.0, value.0)
        }
        OperationKind::CellClear { cell } => format!("cell_clear(v{})", cell.0),
        OperationKind::GlobalGet { dest, name } => {
            format!("v{} = global_get({name:?})", dest.0)
        }
        OperationKind::GlobalSet { name, value } => {
            format!("global_set({name:?}, v{})", value.0)
        }
        OperationKind::GlobalDelete { name } => format!("global_delete({name:?})"),
        OperationKind::ExceptionActive { dest } => format!("v{} = exception_active()", dest.0),
        OperationKind::ExceptionMatches {
            dest,
            exception,
            expected_type,
        } => format!(
            "v{} = exception_matches(v{}, v{})",
            dest.0, exception.0, expected_type.0
        ),
        OperationKind::ExceptionSplit {
            dest,
            exception,
            expected_type,
        } => format!(
            "v{} = exception_split(v{}, v{})",
            dest.0, exception.0, expected_type.0
        ),
        OperationKind::ExceptionSetActive { exception } => {
            format!("exception_set_active(v{})", exception.0)
        }
        OperationKind::ExceptionMerge { remainder } => {
            format!("exception_merge(v{})", remainder.0)
        }
        OperationKind::ExceptionCombine { dest, left, right } => {
            format!("v{} = exception_combine(v{}, v{})", dest.0, left.0, right.0)
        }
        OperationKind::ExceptionClearActive => "exception_clear_active()".to_owned(),
        OperationKind::HandlerEnter { dest } => format!("v{} = handler_enter()", dest.0),
        OperationKind::HandlerLeave => "handler_leave()".to_owned(),
        OperationKind::Raise {
            exception,
            cause,
            suppress_context,
        } => format!(
            "raise v{} cause={cause:?} suppress={suppress_context}",
            exception.0
        ),
        OperationKind::Reraise => "raise".to_owned(),
        OperationKind::Propagate => "propagate_exception()".to_owned(),
        OperationKind::Print { values } => format!(
            "print({})",
            values
                .iter()
                .map(|value| format!("v{}", value.0))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        OperationKind::PrintLiteral { value } => format!("print_literal({value:?})"),
        OperationKind::Collect => "rimera_collect()".to_owned(),
    }
}

fn render_terminator(terminator: &Terminator) -> String {
    match terminator {
        Terminator::Jump { target, arguments } => format!(
            "# jump block {}({})",
            target.0,
            arguments
                .iter()
                .map(|value| format!("v{}", value.0))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Terminator::Branch {
            condition,
            then_target,
            else_target,
            ..
        } => {
            format!(
                "# branch v{} -> block {} / block {}",
                condition.0, then_target.0, else_target.0
            )
        }
        Terminator::Return { code } => format!("# return {code}"),
        Terminator::ReturnValue { value } => format!("# return {value:?}"),
        Terminator::Unreachable => "# unreachable".to_owned(),
    }
}

fn verify_value(value: ValueId, count: u32) -> Result<(), String> {
    if value.0 < count {
        Ok(())
    } else {
        Err(format!("MIR references unknown value %{}", value.0))
    }
}

fn operation_inputs(operation: &OperationKind) -> Vec<ValueId> {
    match operation {
        OperationKind::Constant { .. }
        | OperationKind::GlobalGet { .. }
        | OperationKind::ClosureGet { .. }
        | OperationKind::ExceptionActive { .. }
        | OperationKind::HandlerEnter { .. }
        | OperationKind::HandlerLeave
        | OperationKind::ExceptionClearActive
        | OperationKind::Reraise
        | OperationKind::Propagate
        | OperationKind::GlobalDelete { .. }
        | OperationKind::PrintLiteral { .. }
        | OperationKind::Collect => Vec::new(),
        OperationKind::Copy { source, .. } => vec![*source],
        OperationKind::Unary { operand, .. } => vec![*operand],
        OperationKind::Binary { left, right, .. }
        | OperationKind::InPlace { left, right, .. }
        | OperationKind::Compare { left, right, .. } => {
            vec![*left, *right]
        }
        OperationKind::Print { values } => values.clone(),
        OperationKind::ValueArray { values, .. }
        | OperationKind::Tuple { values, .. }
        | OperationKind::List { values, .. }
        | OperationKind::Set { values, .. } => values.clone(),
        OperationKind::Dictionary { keys, values, .. } => {
            keys.iter().chain(values).copied().collect()
        }
        OperationKind::DictionaryMerge { dictionary, source } => vec![*dictionary, *source],
        OperationKind::SliceNew {
            start, stop, step, ..
        } => start
            .iter()
            .chain(stop.iter())
            .chain(step.iter())
            .copied()
            .collect(),
        OperationKind::Contains {
            collection, needle, ..
        } => vec![*collection, *needle],
        OperationKind::Unpack { value, .. } => vec![*value],
        OperationKind::ItemGet {
            collection, index, ..
        } => vec![*collection, *index],
        OperationKind::Length { value, .. } => vec![*value],
        OperationKind::IteratorNew { value, .. } => vec![*value],
        OperationKind::IteratorNext { iterator, .. } => vec![*iterator],
        OperationKind::Range {
            start, stop, step, ..
        } => vec![*start, *stop, *step],
        OperationKind::ItemSet {
            collection,
            index,
            value,
        } => vec![*collection, *index, *value],
        OperationKind::ItemDelete { collection, index } => vec![*collection, *index],
        OperationKind::ValueArrayGet { array, .. } => vec![*array],
        OperationKind::MakeFunction {
            defaults, closure, ..
        } => defaults
            .iter()
            .map(|(_, value)| *value)
            .chain(closure.iter().copied())
            .collect(),
        OperationKind::ClassNamespaceNew { .. } => Vec::new(),
        OperationKind::ClassNamespaceSet {
            namespace, value, ..
        } => vec![*namespace, *value],
        OperationKind::ClassNamespaceGet { namespace, .. } => vec![*namespace],
        OperationKind::ClassNameGet { namespace, .. } => vec![*namespace],
        OperationKind::ClassNamespaceDelete { namespace, .. } => vec![*namespace],
        OperationKind::ClassNew {
            bases, namespace, ..
        } => bases
            .iter()
            .copied()
            .chain(std::iter::once(*namespace))
            .collect(),
        OperationKind::AttributeGet { receiver, .. }
        | OperationKind::AttributeDelete { receiver, .. } => vec![*receiver],
        OperationKind::AttributeSet {
            receiver, value, ..
        } => vec![*receiver, *value],
        OperationKind::Call {
            callable,
            positional,
            keywords,
            ..
        } => std::iter::once(*callable)
            .chain(positional.iter().copied())
            .chain(keywords.iter().map(|(_, value)| *value))
            .collect(),
        OperationKind::CallArgumentsNew { callable, .. } => vec![*callable],
        OperationKind::CallArgumentAdd {
            arguments, value, ..
        } => vec![*arguments, *value],
        OperationKind::CallPrepared {
            callable, arguments, ..
        } => vec![*callable, *arguments],
        OperationKind::CallModuleChunk { .. } => Vec::new(),
        OperationKind::CellNew { initial, .. } => initial.iter().copied().collect(),
        OperationKind::CellGet { cell, .. } => vec![*cell],
        OperationKind::CellSet { cell, value } => vec![*cell, *value],
        OperationKind::CellClear { cell } => vec![*cell],
        OperationKind::GlobalSet { value, .. } => vec![*value],
        OperationKind::ExceptionMatches {
            exception,
            expected_type,
            ..
        } => vec![*exception, *expected_type],
        OperationKind::ExceptionSplit {
            exception,
            expected_type,
            ..
        } => vec![*exception, *expected_type],
        OperationKind::ExceptionSetActive { exception } => vec![*exception],
        OperationKind::ExceptionMerge { remainder } => vec![*remainder],
        OperationKind::ExceptionCombine { left, right, .. } => vec![*left, *right],
        OperationKind::Raise {
            exception, cause, ..
        } => std::iter::once(*exception)
            .chain(cause.iter().copied())
            .collect(),
    }
}

fn verify_terminator(
    program: &Function,
    block_index: usize,
    terminator: &Terminator,
) -> Result<(), String> {
    match terminator {
        Terminator::Jump { target, arguments } => {
            verify_edge(program, block_index, *target, arguments)
        }
        Terminator::Branch {
            condition,
            then_target,
            then_arguments,
            else_target,
            else_arguments,
        } => {
            verify_value(*condition, program.value_count)?;
            verify_edge(program, block_index, *then_target, then_arguments)?;
            verify_edge(program, block_index, *else_target, else_arguments)
        }
        Terminator::Return { .. } => Ok(()),
        Terminator::ReturnValue { value } => {
            if let Some(value) = value {
                verify_value(*value, program.value_count)?;
            }
            Ok(())
        }
        Terminator::Unreachable => Err(format!("MIR block {block_index} is unterminated")),
    }
}

fn verify_edge(
    program: &Function,
    from: usize,
    target: BlockId,
    arguments: &[ValueId],
) -> Result<(), String> {
    let Some(block) = program.blocks.get(target.0 as usize) else {
        return Err(format!(
            "MIR block {from} targets missing block {}",
            target.0
        ));
    };
    if block.parameters.len() != arguments.len() {
        return Err(format!(
            "MIR edge {from} -> {} supplies {} arguments for {} parameters",
            target.0,
            arguments.len(),
            block.parameters.len()
        ));
    }
    for argument in arguments {
        verify_value(*argument, program.value_count)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifier_rejects_wrong_edge_arity() {
        let program = Function {
            kind: FunctionKind::Python,
            name: "test".to_owned(),
            qualified_name: "test".to_owned(),
            parameters: vec![],
            entry: BlockId(0),
            value_count: 1,
            exception_edges: BTreeMap::new(),
            blocks: vec![
                Block {
                    parameters: vec![ValueId(0)],
                    operations: vec![],
                    terminator: Terminator::Jump {
                        target: BlockId(1),
                        arguments: vec![],
                    },
                },
                Block {
                    parameters: vec![ValueId(0)],
                    operations: vec![],
                    terminator: Terminator::Return { code: 0 },
                },
            ],
        };
        assert!(verify_function(&program).is_err());
    }

    #[test]
    fn verifier_accepts_a_well_formed_block_argument() {
        let program = Function {
            kind: FunctionKind::Python,
            name: "test".to_owned(),
            qualified_name: "test".to_owned(),
            parameters: vec![],
            entry: BlockId(0),
            value_count: 2,
            exception_edges: BTreeMap::new(),
            blocks: vec![
                Block {
                    parameters: vec![],
                    operations: vec![Operation {
                        span: Span::default(),
                        kind: OperationKind::Constant {
                            dest: ValueId(0),
                            value: Constant::Bool(true),
                        },
                    }],
                    terminator: Terminator::Jump {
                        target: BlockId(1),
                        arguments: vec![ValueId(0)],
                    },
                },
                Block {
                    parameters: vec![ValueId(1)],
                    operations: vec![],
                    terminator: Terminator::Return { code: 0 },
                },
            ],
        };
        assert_eq!(verify_function(&program), Ok(()));
    }

    #[test]
    fn verifier_rejects_duplicate_value_definitions() {
        let program = Function {
            kind: FunctionKind::Python,
            name: "test".to_owned(),
            qualified_name: "test".to_owned(),
            parameters: vec![],
            entry: BlockId(0),
            value_count: 1,
            exception_edges: BTreeMap::new(),
            blocks: vec![Block {
                parameters: vec![],
                operations: vec![
                    Operation {
                        span: Span::default(),
                        kind: OperationKind::Constant {
                            dest: ValueId(0),
                            value: Constant::None,
                        },
                    },
                    Operation {
                        span: Span::default(),
                        kind: OperationKind::Constant {
                            dest: ValueId(0),
                            value: Constant::Bool(false),
                        },
                    },
                ],
                terminator: Terminator::Return { code: 0 },
            }],
        };
        assert!(
            verify_function(&program)
                .unwrap_err()
                .contains("defined more than once")
        );
    }

    #[test]
    fn verifier_rejects_missing_target_blocks() {
        let program = Function {
            kind: FunctionKind::Python,
            name: "test".to_owned(),
            qualified_name: "test".to_owned(),
            parameters: vec![],
            entry: BlockId(0),
            value_count: 0,
            exception_edges: BTreeMap::new(),
            blocks: vec![Block {
                parameters: vec![],
                operations: vec![],
                terminator: Terminator::Jump {
                    target: BlockId(9),
                    arguments: vec![],
                },
            }],
        };
        assert!(
            verify_function(&program)
                .unwrap_err()
                .contains("missing block 9")
        );
    }

    #[test]
    fn safepoint_liveness_excludes_dead_values() {
        let program = Function {
            kind: FunctionKind::Python,
            name: "test".to_owned(),
            qualified_name: "test".to_owned(),
            parameters: vec![],
            entry: BlockId(0),
            value_count: 2,
            exception_edges: BTreeMap::new(),
            blocks: vec![Block {
                parameters: vec![],
                operations: vec![
                    Operation {
                        span: Span::default(),
                        kind: OperationKind::Constant {
                            dest: ValueId(0),
                            value: Constant::String("dead".to_owned()),
                        },
                    },
                    Operation {
                        span: Span::default(),
                        kind: OperationKind::Constant {
                            dest: ValueId(1),
                            value: Constant::String("live".to_owned()),
                        },
                    },
                    Operation {
                        span: Span::default(),
                        kind: OperationKind::Collect,
                    },
                    Operation {
                        span: Span::default(),
                        kind: OperationKind::Print {
                            values: vec![ValueId(1)],
                        },
                    },
                ],
                terminator: Terminator::Return { code: 0 },
            }],
        };
        let plan = safepoint_plan(&program).unwrap();
        assert_eq!(plan.operation_roots(0, 0), Some([].as_slice()));
        assert_eq!(plan.operation_roots(0, 1), Some([].as_slice()));
        assert_eq!(plan.operation_roots(0, 2), Some([ValueId(1)].as_slice()));
        assert_eq!(plan.operation_roots(0, 3), Some([ValueId(1)].as_slice()));
        assert_eq!(plan.max_roots(), 1);
    }

    #[test]
    fn class_construction_roots_ordered_bases_and_namespace() {
        let operations = vec![
            Operation {
                span: Span::default(),
                kind: OperationKind::Constant {
                    dest: ValueId(0),
                    value: Constant::None,
                },
            },
            Operation {
                span: Span::default(),
                kind: OperationKind::Constant {
                    dest: ValueId(1),
                    value: Constant::None,
                },
            },
            Operation {
                span: Span::default(),
                kind: OperationKind::ClassNamespaceNew { dest: ValueId(2) },
            },
            Operation {
                span: Span::default(),
                kind: OperationKind::ClassNew {
                    dest: ValueId(3),
                    name: "Derived".to_owned(),
                    bases: vec![ValueId(0), ValueId(1)],
                    namespace: ValueId(2),
                },
            },
        ];
        let program = Function {
            kind: FunctionKind::Python,
            name: "test".to_owned(),
            qualified_name: "test".to_owned(),
            parameters: vec![],
            entry: BlockId(0),
            value_count: 4,
            exception_edges: BTreeMap::from([((0, 3), BlockId(1))]),
            blocks: vec![
                Block {
                    parameters: vec![],
                    operations,
                    terminator: Terminator::Return { code: 0 },
                },
                Block {
                    parameters: vec![],
                    operations: vec![],
                    terminator: Terminator::Return { code: 1 },
                },
            ],
        };
        verify_function(&program).unwrap();
        let plan = safepoint_plan(&program).unwrap();
        assert_eq!(
            plan.operation_roots(0, 3),
            Some([ValueId(0), ValueId(1), ValueId(2)].as_slice())
        );
        assert_eq!(
            operation_inputs(&program.blocks[0].operations[3].kind),
            vec![ValueId(0), ValueId(1), ValueId(2)]
        );
    }

    #[test]
    fn branch_safepoint_roots_condition_and_edge_values() {
        let program = Function {
            kind: FunctionKind::Python,
            name: "test".to_owned(),
            qualified_name: "test".to_owned(),
            parameters: vec![],
            entry: BlockId(0),
            value_count: 4,
            exception_edges: BTreeMap::new(),
            blocks: vec![
                Block {
                    parameters: vec![],
                    operations: vec![
                        Operation {
                            span: Span::default(),
                            kind: OperationKind::Constant {
                                dest: ValueId(0),
                                value: Constant::String("value".to_owned()),
                            },
                        },
                        Operation {
                            span: Span::default(),
                            kind: OperationKind::Constant {
                                dest: ValueId(1),
                                value: Constant::Bool(true),
                            },
                        },
                    ],
                    terminator: Terminator::Branch {
                        condition: ValueId(1),
                        then_target: BlockId(1),
                        then_arguments: vec![ValueId(0)],
                        else_target: BlockId(2),
                        else_arguments: vec![ValueId(0)],
                    },
                },
                Block {
                    parameters: vec![ValueId(2)],
                    operations: vec![Operation {
                        span: Span::default(),
                        kind: OperationKind::Print {
                            values: vec![ValueId(2)],
                        },
                    }],
                    terminator: Terminator::Return { code: 0 },
                },
                Block {
                    parameters: vec![ValueId(3)],
                    operations: vec![Operation {
                        span: Span::default(),
                        kind: OperationKind::Print {
                            values: vec![ValueId(3)],
                        },
                    }],
                    terminator: Terminator::Return { code: 0 },
                },
            ],
        };
        let plan = safepoint_plan(&program).unwrap();
        assert_eq!(
            plan.terminator_roots(0),
            Some([ValueId(0), ValueId(1)].as_slice())
        );
    }

    #[test]
    fn loop_safepoint_preserves_the_loop_carried_value() {
        let program = Function {
            kind: FunctionKind::Python,
            name: "test".to_owned(),
            qualified_name: "test".to_owned(),
            parameters: vec![],
            entry: BlockId(0),
            value_count: 4,
            exception_edges: BTreeMap::new(),
            blocks: vec![
                Block {
                    parameters: vec![],
                    operations: vec![Operation {
                        span: Span::default(),
                        kind: OperationKind::Constant {
                            dest: ValueId(0),
                            value: Constant::String("loop".to_owned()),
                        },
                    }],
                    terminator: Terminator::Jump {
                        target: BlockId(1),
                        arguments: vec![ValueId(0)],
                    },
                },
                Block {
                    parameters: vec![ValueId(1)],
                    operations: vec![Operation {
                        span: Span::default(),
                        kind: OperationKind::Constant {
                            dest: ValueId(2),
                            value: Constant::Bool(false),
                        },
                    }],
                    terminator: Terminator::Branch {
                        condition: ValueId(2),
                        then_target: BlockId(1),
                        then_arguments: vec![ValueId(1)],
                        else_target: BlockId(2),
                        else_arguments: vec![ValueId(1)],
                    },
                },
                Block {
                    parameters: vec![ValueId(3)],
                    operations: vec![Operation {
                        span: Span::default(),
                        kind: OperationKind::Print {
                            values: vec![ValueId(3)],
                        },
                    }],
                    terminator: Terminator::Return { code: 0 },
                },
            ],
        };
        let plan = safepoint_plan(&program).unwrap();
        assert_eq!(
            plan.terminator_roots(1),
            Some([ValueId(1), ValueId(2)].as_slice())
        );
    }

    #[test]
    fn later_safepoint_can_use_fewer_shadow_roots() {
        let program = Function {
            kind: FunctionKind::Python,
            name: "test".to_owned(),
            qualified_name: "test".to_owned(),
            parameters: vec![],
            entry: BlockId(0),
            value_count: 2,
            exception_edges: BTreeMap::new(),
            blocks: vec![Block {
                parameters: vec![],
                operations: vec![
                    Operation {
                        span: Span::default(),
                        kind: OperationKind::Constant {
                            dest: ValueId(0),
                            value: Constant::String("first".to_owned()),
                        },
                    },
                    Operation {
                        span: Span::default(),
                        kind: OperationKind::Constant {
                            dest: ValueId(1),
                            value: Constant::String("second".to_owned()),
                        },
                    },
                    Operation {
                        span: Span::default(),
                        kind: OperationKind::Print {
                            values: vec![ValueId(0), ValueId(1)],
                        },
                    },
                    Operation {
                        span: Span::default(),
                        kind: OperationKind::Print {
                            values: vec![ValueId(1)],
                        },
                    },
                ],
                terminator: Terminator::Return { code: 0 },
            }],
        };
        let plan = safepoint_plan(&program).unwrap();
        assert_eq!(plan.operation_roots(0, 2).unwrap().len(), 2);
        assert_eq!(plan.operation_roots(0, 3), Some([ValueId(1)].as_slice()));
        assert_eq!(plan.max_roots(), 2);
    }
}
