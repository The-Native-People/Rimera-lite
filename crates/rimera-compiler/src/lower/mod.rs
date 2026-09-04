use std::collections::BTreeMap;

use crate::core::Span;
use crate::{hir, mir};

// Module chunks are internal execution units. Keeping their source suites
// bounded prevents target backends from imposing a whole-module code-size
// ceiling while preserving statement boundaries and module-global semantics.
const MODULE_CHUNK_STATEMENTS: usize = 512;

type TypeParameterCells = Vec<(String, Option<mir::ValueId>)>;
type InstalledTypeParameters = (Vec<mir::ValueId>, TypeParameterCells);

pub fn lower(module: &hir::Module) -> Result<mir::Program, String> {
    let mut program = ProgramLowerer {
        functions: Vec::new(),
    };
    let entry = if module.statements.len() <= MODULE_CHUNK_STATEMENTS {
        program.lower_scope(
            "<module>".to_owned(),
            "<module>".to_owned(),
            &[],
            &[],
            &[],
            &module.statements,
            true,
        )?
    } else {
        let mut chunks = Vec::new();
        for statements in module.statements.chunks(MODULE_CHUNK_STATEMENTS) {
            let function = program.lower_scope(
                "<module>".to_owned(),
                "<module>".to_owned(),
                &[],
                &[],
                &[],
                statements,
                false,
            )?;
            let span = statements
                .first()
                .map_or(Span::default(), |statement| statement.span);
            chunks.push((function, span));
        }
        program.lower_module_driver(&chunks, statements_need_annotations(&module.statements))?
    };
    let functions = program
        .functions
        .into_iter()
        .map(|function| function.ok_or_else(|| "MIR function was not completed".to_owned()))
        .collect::<Result<Vec<_>, _>>()?;
    let program = mir::Program {
        filename: module.filename.clone(),
        line_starts: module.line_starts.clone(),
        entry,
        functions,
    };
    mir::verify(&program)?;
    Ok(program)
}

fn statements_contain_yield(statements: &[hir::Statement]) -> bool {
    statements.iter().any(statement_contains_yield)
}

fn statement_contains_yield(statement: &hir::Statement) -> bool {
    match &statement.kind {
        hir::StatementKind::Assign { targets, value } => {
            targets.iter().any(target_contains_yield) || expression_contains_yield(value)
        }
        hir::StatementKind::AugAssign { target, value, .. } => {
            target_contains_yield(target) || expression_contains_yield(value)
        }
        hir::StatementKind::Delete { targets } => targets.iter().any(target_contains_yield),
        hir::StatementKind::AnnAssign {
            target,
            annotation,
            value,
            ..
        } => {
            target_contains_yield(target)
                || expression_contains_yield(annotation)
                || value.as_ref().is_some_and(expression_contains_yield)
        }
        hir::StatementKind::Assert { test, message } => {
            expression_contains_yield(test)
                || message.as_ref().is_some_and(expression_contains_yield)
        }
        hir::StatementKind::FunctionDef {
            decorators,
            parameters,
            return_annotation,
            ..
        } => {
            decorators.iter().any(expression_contains_yield)
                || parameters.iter().any(|parameter| {
                    parameter
                        .default
                        .as_ref()
                        .is_some_and(expression_contains_yield)
                        || parameter
                            .annotation
                            .as_ref()
                            .is_some_and(expression_contains_yield)
                })
                || return_annotation
                    .as_ref()
                    .is_some_and(expression_contains_yield)
        }
        hir::StatementKind::ClassDef {
            decorators,
            bases,
            metaclass,
            keywords,
            ..
        } => {
            decorators.iter().any(expression_contains_yield)
                || bases.iter().any(expression_contains_yield)
                || metaclass.as_ref().is_some_and(expression_contains_yield)
                || keywords
                    .iter()
                    .any(|(_, value)| expression_contains_yield(value))
        }
        hir::StatementKind::TypeAlias { value, .. } => expression_contains_yield(value),
        hir::StatementKind::Return { value } => {
            value.as_ref().is_some_and(expression_contains_yield)
        }
        hir::StatementKind::Import { .. }
        | hir::StatementKind::Break
        | hir::StatementKind::Continue => false,
        hir::StatementKind::Expression(value) => expression_contains_yield(value),
        hir::StatementKind::Raise { exception, cause } => {
            exception.as_ref().is_some_and(expression_contains_yield)
                || cause.as_ref().is_some_and(expression_contains_yield)
        }
        hir::StatementKind::Try {
            body,
            handlers,
            else_body,
            finally_body,
            ..
        } => {
            statements_contain_yield(body)
                || handlers.iter().any(|handler| {
                    handler
                        .exception_type
                        .as_ref()
                        .is_some_and(expression_contains_yield)
                        || statements_contain_yield(&handler.body)
                })
                || statements_contain_yield(else_body)
                || statements_contain_yield(finally_body)
        }
        hir::StatementKind::Print { values } => values.iter().any(expression_contains_yield),
        hir::StatementKind::If {
            condition,
            then_body,
            else_body,
        } => {
            expression_contains_yield(condition)
                || statements_contain_yield(then_body)
                || statements_contain_yield(else_body)
        }
        hir::StatementKind::While { condition, body } => {
            expression_contains_yield(condition) || statements_contain_yield(body)
        }
        hir::StatementKind::For {
            target,
            iterable,
            body,
            else_body,
        } => {
            target_contains_yield(target)
                || expression_contains_yield(iterable)
                || statements_contain_yield(body)
                || statements_contain_yield(else_body)
        }
        hir::StatementKind::Match { subject, cases } => {
            expression_contains_yield(subject)
                || cases.iter().any(|case| {
                    case.guard.as_ref().is_some_and(expression_contains_yield)
                        || statements_contain_yield(&case.body)
                })
        }
    }
}

fn target_contains_yield(target: &hir::Target) -> bool {
    match &target.kind {
        hir::TargetKind::Name { .. } => false,
        hir::TargetKind::Attribute { receiver, .. } => expression_contains_yield(receiver),
        hir::TargetKind::Item { collection, index } => {
            expression_contains_yield(collection) || expression_contains_yield(index)
        }
        hir::TargetKind::Sequence { elements, .. } => elements.iter().any(target_contains_yield),
        hir::TargetKind::Starred(target) => target_contains_yield(target),
    }
}

fn expression_contains_yield(expression: &hir::Expression) -> bool {
    match &expression.kind {
        hir::ExpressionKind::Yield { .. } | hir::ExpressionKind::YieldFrom { .. } => true,
        hir::ExpressionKind::Slice { start, stop, step } => [start, stop, step]
            .into_iter()
            .flatten()
            .any(|value| expression_contains_yield(value)),
        hir::ExpressionKind::List(values)
        | hir::ExpressionKind::Tuple(values)
        | hir::ExpressionKind::Set(values)
        | hir::ExpressionKind::JoinedString(values)
        | hir::ExpressionKind::Boolean { values, .. } => {
            values.iter().any(expression_contains_yield)
        }
        hir::ExpressionKind::Dictionary(entries) => entries.iter().any(|entry| match entry {
            hir::DictionaryEntry::Pair { key, value } => {
                expression_contains_yield(key) || expression_contains_yield(value)
            }
            hir::DictionaryEntry::Unpack(value) => expression_contains_yield(value),
        }),
        hir::ExpressionKind::Subscript { value, index } => {
            expression_contains_yield(value) || expression_contains_yield(index)
        }
        hir::ExpressionKind::Attribute { value, .. }
        | hir::ExpressionKind::Length { value }
        | hir::ExpressionKind::Unary { operand: value, .. } => expression_contains_yield(value),
        hir::ExpressionKind::Range { start, stop, step } => {
            expression_contains_yield(start)
                || expression_contains_yield(stop)
                || expression_contains_yield(step)
        }
        hir::ExpressionKind::Lambda { .. } => false,
        hir::ExpressionKind::Binary { left, right, .. } => {
            expression_contains_yield(left) || expression_contains_yield(right)
        }
        hir::ExpressionKind::Compare { left, comparisons } => {
            expression_contains_yield(left)
                || comparisons
                    .iter()
                    .any(|(_, right)| expression_contains_yield(right))
        }
        hir::ExpressionKind::NamedExpression { value, .. } => expression_contains_yield(value),
        hir::ExpressionKind::Comprehension { outer_iterable, .. } => {
            expression_contains_yield(outer_iterable)
        }
        hir::ExpressionKind::FormattedValue {
            value, format_spec, ..
        } => {
            expression_contains_yield(value)
                || format_spec
                    .as_ref()
                    .is_some_and(|value| expression_contains_yield(value))
        }
        hir::ExpressionKind::Call { callable, parts } => {
            expression_contains_yield(callable)
                || parts.iter().any(|part| match part {
                    hir::CallPart::Positional(value)
                    | hir::CallPart::Starred(value)
                    | hir::CallPart::Keyword { value, .. }
                    | hir::CallPart::KeywordUnpack(value) => expression_contains_yield(value),
                })
        }
        hir::ExpressionKind::None
        | hir::ExpressionKind::Bool(_)
        | hir::ExpressionKind::Int(_)
        | hir::ExpressionKind::Float(_)
        | hir::ExpressionKind::String(_)
        | hir::ExpressionKind::Bytes(_)
        | hir::ExpressionKind::Complex { .. }
        | hir::ExpressionKind::Name { .. } => false,
    }
}

struct ProgramLowerer {
    functions: Vec<Option<mir::Function>>,
}

impl ProgramLowerer {
    fn lower_class_body(
        &mut self,
        name: String,
        qualified_name: String,
        scope_qualified_name: String,
        body: &[hir::ClassMember],
        free: &[String],
    ) -> Result<mir::FunctionId, String> {
        let id = mir::FunctionId(
            u32::try_from(self.functions.len()).map_err(|_| "too many MIR functions")?,
        );
        self.functions.push(None);
        let mut lowerer = Lowerer::new(self);
        lowerer.qualname_prefix = Some(scope_qualified_name);
        lowerer.nested_uses_locals = false;
        let namespace = lowerer.value();
        let parameter = mir::Parameter {
            value: namespace,
            name: "__rimera_namespace".to_owned(),
            kind: rimera_abi::RParameterKind::PositionalOnly,
            has_default: false,
        };
        for (index, name) in free.iter().enumerate() {
            let cell = lowerer.value();
            lowerer.emit(
                Span::default(),
                mir::OperationKind::ClosureGet {
                    dest: cell,
                    index: u32::try_from(index).map_err(|_| "too many closure cells")?,
                },
            );
            lowerer.cells.insert(name.clone(), cell);
        }
        lowerer.emit(
            Span::default(),
            mir::OperationKind::ReflectionScopeConfigure {
                namespace: Some(namespace),
                comprehension: false,
            },
        );
        lowerer.class_scopes.push(ClassScope { namespace });
        if class_members_need_annotations(body) {
            lowerer.emit(
                Span::default(),
                mir::OperationKind::AnnotationsEnsure {
                    namespace: Some(namespace),
                },
            );
        }
        if class_members_need_class_cell(body) {
            let cell = lowerer.value();
            lowerer.emit(
                Span::default(),
                mir::OperationKind::CellNew {
                    dest: cell,
                    initial: None,
                },
            );
            lowerer.cells.insert("__class__".to_owned(), cell);
            lowerer.emit(
                Span::default(),
                mir::OperationKind::ClassNamespaceSet {
                    namespace,
                    name: "__classcell__".to_owned(),
                    value: cell,
                },
            );
        }
        lowerer.lower_class_assignment_members(Span::default(), namespace, body)?;
        lowerer.class_scopes.pop();
        if lowerer.is_open() {
            lowerer.terminate(mir::Terminator::ReturnValue { value: None })?;
        }
        self.functions[id.0 as usize] = Some(mir::Function {
            kind: mir::FunctionKind::ClassBody,
            name,
            qualified_name,
            parameters: vec![parameter],
            entry: mir::BlockId(0),
            blocks: lowerer.blocks,
            value_count: lowerer.next_value,
            exception_edges: lowerer.exception_edges,
        });
        Ok(id)
    }

    fn lower_module_driver(
        &mut self,
        chunks: &[(mir::FunctionId, Span)],
        has_annotations: bool,
    ) -> Result<mir::FunctionId, String> {
        let id = mir::FunctionId(
            u32::try_from(self.functions.len()).map_err(|_| "too many MIR functions")?,
        );
        self.functions.push(None);
        let mut lowerer = Lowerer::new(self);
        lowerer.module_semantics = true;
        if has_annotations {
            lowerer.emit(
                Span::default(),
                mir::OperationKind::AnnotationsEnsure { namespace: None },
            );
        }
        for (function, span) in chunks {
            lowerer.emit(
                *span,
                mir::OperationKind::CallModuleChunk {
                    function: *function,
                },
            );
        }
        lowerer.terminate(mir::Terminator::Return { code: 0 })?;
        self.functions[id.0 as usize] = Some(mir::Function {
            kind: mir::FunctionKind::Module,
            name: "<module>".to_owned(),
            qualified_name: "<module>".to_owned(),
            parameters: vec![],
            entry: mir::BlockId(0),
            blocks: lowerer.blocks,
            value_count: lowerer.next_value,
            exception_edges: lowerer.exception_edges,
        });
        Ok(id)
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_scope(
        &mut self,
        name: String,
        qualified_name: String,
        parameters: &[hir::Parameter],
        locals: &[String],
        free: &[String],
        statements: &[hir::Statement],
        module_scope: bool,
    ) -> Result<mir::FunctionId, String> {
        let id = mir::FunctionId(
            u32::try_from(self.functions.len()).map_err(|_| "too many MIR functions")?,
        );
        self.functions.push(None);
        let source_generator =
            !module_scope && name != "<module>" && statements_contain_yield(statements);
        let mut lowerer = Lowerer::new(self);
        lowerer.module_semantics = name == "<module>";
        if !module_scope {
            lowerer.qualname_prefix = Some(qualified_name.clone());
            lowerer.nested_uses_locals = true;
        }
        let mut mir_parameters = Vec::with_capacity(parameters.len());
        let mut parameter_values = BTreeMap::new();
        for parameter in parameters {
            let value = lowerer.value();
            parameter_values.insert(parameter.name.clone(), value);
            mir_parameters.push(mir::Parameter {
                value,
                name: parameter.name.clone(),
                kind: parameter.kind,
                has_default: parameter.default.is_some(),
            });
        }
        if !module_scope && name != "<module>" {
            for (index, name) in free.iter().enumerate() {
                let cell = lowerer.value();
                lowerer.emit(
                    Span::default(),
                    mir::OperationKind::ClosureGet {
                        dest: cell,
                        index: u32::try_from(index).map_err(|_| "too many closure cells")?,
                    },
                );
                lowerer.cells.insert(name.clone(), cell);
            }
            for local in locals {
                let cell = lowerer.value();
                lowerer.emit(
                    Span::default(),
                    mir::OperationKind::CellNew {
                        dest: cell,
                        initial: parameter_values.get(local).copied(),
                    },
                );
                lowerer.cells.insert(local.clone(), cell);
            }
            lowerer.emit(
                Span::default(),
                mir::OperationKind::ReflectionScopeConfigure {
                    namespace: None,
                    comprehension: false,
                },
            );
            let mut registered = Vec::new();
            for name in parameters
                .iter()
                .map(|parameter| &parameter.name)
                .chain(locals.iter())
                .chain(free.iter())
            {
                if registered.contains(name) {
                    continue;
                }
                let Some(cell) = lowerer.cells.get(name).copied() else {
                    continue;
                };
                lowerer.emit(
                    Span::default(),
                    mir::OperationKind::ReflectionLocalRegister {
                        name: name.clone(),
                        cell,
                    },
                );
                registered.push(name.clone());
            }
        }
        if module_scope && statements_need_annotations(statements) {
            lowerer.emit(
                Span::default(),
                mir::OperationKind::AnnotationsEnsure { namespace: None },
            );
        }
        let entry = lowerer.current;
        lowerer.statements(statements)?;
        if lowerer.is_open() {
            lowerer.terminate(if module_scope {
                mir::Terminator::Return { code: 0 }
            } else {
                mir::Terminator::ReturnValue { value: None }
            })?;
        }
        let function = mir::Function {
            kind: if module_scope {
                mir::FunctionKind::Module
            } else if source_generator {
                mir::FunctionKind::Generator
            } else {
                mir::FunctionKind::Python
            },
            name,
            qualified_name,
            parameters: mir_parameters,
            entry,
            blocks: lowerer.blocks,
            value_count: lowerer.next_value,
            exception_edges: lowerer.exception_edges,
        };
        self.functions[id.0 as usize] = Some(function);
        Ok(id)
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_comprehension(
        &mut self,
        kind: hir::ComprehensionKind,
        element: &hir::Expression,
        key: Option<&hir::Expression>,
        clauses: &[hir::ComprehensionClause],
        locals: &[String],
        free: &[String],
        qualname_prefix: Option<String>,
        nested_uses_locals: bool,
    ) -> Result<mir::FunctionId, String> {
        let id = mir::FunctionId(
            u32::try_from(self.functions.len()).map_err(|_| "too many MIR functions")?,
        );
        self.functions.push(None);
        let mut lowerer = Lowerer::new(self);
        lowerer.qualname_prefix = qualname_prefix;
        lowerer.nested_uses_locals = nested_uses_locals;
        let outer_iterator = lowerer.value();
        let parameter = mir::Parameter {
            value: outer_iterator,
            name: ".0".to_owned(),
            kind: rimera_abi::RParameterKind::PositionalOnly,
            has_default: false,
        };
        for (index, name) in free.iter().enumerate() {
            let cell = lowerer.value();
            lowerer.emit(
                Span::default(),
                mir::OperationKind::ClosureGet {
                    dest: cell,
                    index: u32::try_from(index).map_err(|_| "too many closure cells")?,
                },
            );
            lowerer.cells.insert(name.clone(), cell);
        }
        for local in locals {
            let cell = lowerer.value();
            lowerer.emit(
                Span::default(),
                mir::OperationKind::CellNew {
                    dest: cell,
                    initial: None,
                },
            );
            lowerer.cells.insert(local.clone(), cell);
        }
        lowerer.emit(
            Span::default(),
            mir::OperationKind::ReflectionScopeConfigure {
                namespace: None,
                comprehension: kind != hir::ComprehensionKind::Generator,
            },
        );
        if kind == hir::ComprehensionKind::Generator {
            let iterator_cell = lowerer.value();
            lowerer.emit(
                Span::default(),
                mir::OperationKind::CellNew {
                    dest: iterator_cell,
                    initial: Some(outer_iterator),
                },
            );
            lowerer.emit(
                Span::default(),
                mir::OperationKind::ReflectionLocalRegister {
                    name: ".0".to_owned(),
                    cell: iterator_cell,
                },
            );
        }
        for local in locals {
            let cell = lowerer
                .cells
                .get(local)
                .copied()
                .ok_or_else(|| format!("comprehension local `{local}` has no cell"))?;
            lowerer.emit(
                Span::default(),
                mir::OperationKind::ReflectionLocalRegister {
                    name: local.clone(),
                    cell,
                },
            );
        }
        if kind == hir::ComprehensionKind::Generator {
            for name in free {
                let cell = lowerer
                    .cells
                    .get(name)
                    .copied()
                    .ok_or_else(|| format!("generator free variable `{name}` has no cell"))?;
                lowerer.emit(
                    Span::default(),
                    mir::OperationKind::ReflectionLocalRegister {
                        name: name.clone(),
                        cell,
                    },
                );
            }
        }
        let result = lowerer.value();
        let (name, initial) = match kind {
            hir::ComprehensionKind::List => (
                "<listcomp>",
                Some(mir::OperationKind::List {
                    dest: result,
                    values: Vec::new(),
                }),
            ),
            hir::ComprehensionKind::Set => (
                "<setcomp>",
                Some(mir::OperationKind::Set {
                    dest: result,
                    values: Vec::new(),
                }),
            ),
            hir::ComprehensionKind::Dictionary => (
                "<dictcomp>",
                Some(mir::OperationKind::Dictionary {
                    dest: result,
                    keys: Vec::new(),
                    values: Vec::new(),
                }),
            ),
            hir::ComprehensionKind::Generator => (
                "<genexpr>",
                Some(mir::OperationKind::Constant {
                    dest: result,
                    value: mir::Constant::None,
                }),
            ),
        };
        if let Some(initial) = initial {
            lowerer.emit(Span::default(), initial);
        }
        let finish = lowerer.new_block();
        lowerer.lower_comprehension_level(
            kind,
            element,
            key,
            clauses,
            0,
            Some(outer_iterator),
            result,
            finish,
        )?;
        lowerer.current = finish;
        lowerer.terminate(mir::Terminator::ReturnValue {
            value: (kind != hir::ComprehensionKind::Generator).then_some(result),
        })?;
        self.functions[id.0 as usize] = Some(mir::Function {
            kind: if kind == hir::ComprehensionKind::Generator {
                mir::FunctionKind::Generator
            } else {
                mir::FunctionKind::Python
            },
            name: name.to_owned(),
            qualified_name: name.to_owned(),
            parameters: vec![parameter],
            entry: mir::BlockId(0),
            blocks: lowerer.blocks,
            value_count: lowerer.next_value,
            exception_edges: lowerer.exception_edges,
        });
        Ok(id)
    }
}

struct Lowerer<'a> {
    program: &'a mut ProgramLowerer,
    blocks: Vec<mir::Block>,
    current: mir::BlockId,
    next_value: u32,
    cells: BTreeMap<String, mir::ValueId>,
    exception_edges: BTreeMap<(u32, u32), mir::BlockId>,
    exception_target: Option<mir::BlockId>,
    cleanups: Vec<CleanupAction>,
    loops: Vec<LoopTargets>,
    class_scopes: Vec<ClassScope>,
    module_semantics: bool,
    qualname_prefix: Option<String>,
    nested_uses_locals: bool,
}

#[derive(Clone)]
enum CleanupAction {
    Handler {
        binding: Option<(String, hir::Binding)>,
        exception_target: Option<mir::BlockId>,
    },
    Finally {
        statements: Vec<hir::Statement>,
        exception_target: Option<mir::BlockId>,
    },
    ClassFinally {
        namespace: mir::ValueId,
        members: Vec<hir::ClassMember>,
        exception_target: Option<mir::BlockId>,
    },
    ClassHandler {
        binding: Option<(String, hir::Binding)>,
        exception_target: Option<mir::BlockId>,
    },
}

#[derive(Clone, Copy)]
struct LoopTargets {
    continue_target: mir::BlockId,
    break_target: mir::BlockId,
    cleanup_depth: usize,
}

struct ClassScope {
    namespace: mir::ValueId,
}

fn statements_need_annotations(statements: &[hir::Statement]) -> bool {
    statements.iter().any(|statement| match &statement.kind {
        hir::StatementKind::AnnAssign { .. } => true,
        hir::StatementKind::If {
            then_body,
            else_body,
            ..
        } => statements_need_annotations(then_body) || statements_need_annotations(else_body),
        hir::StatementKind::While { body, .. } => statements_need_annotations(body),
        hir::StatementKind::For {
            body, else_body, ..
        } => statements_need_annotations(body) || statements_need_annotations(else_body),
        hir::StatementKind::Try {
            body,
            handlers,
            else_body,
            finally_body,
            ..
        } => {
            statements_need_annotations(body)
                || handlers
                    .iter()
                    .any(|handler| statements_need_annotations(&handler.body))
                || statements_need_annotations(else_body)
                || statements_need_annotations(finally_body)
        }
        hir::StatementKind::FunctionDef { .. } | hir::StatementKind::ClassDef { .. } => false,
        _ => false,
    })
}

fn class_members_need_annotations(members: &[hir::ClassMember]) -> bool {
    members.iter().any(|member| match member {
        hir::ClassMember::AnnAssign { .. } => true,
        hir::ClassMember::If {
            then_body,
            else_body,
            ..
        } => class_members_need_annotations(then_body) || class_members_need_annotations(else_body),
        hir::ClassMember::While { body, .. } => class_members_need_annotations(body),
        hir::ClassMember::For {
            body, else_body, ..
        } => class_members_need_annotations(body) || class_members_need_annotations(else_body),
        hir::ClassMember::Try {
            body,
            handlers,
            else_body,
            finally_body,
            ..
        } => {
            class_members_need_annotations(body)
                || handlers
                    .iter()
                    .any(|handler| class_members_need_annotations(&handler.body))
                || class_members_need_annotations(else_body)
                || class_members_need_annotations(finally_body)
        }
        hir::ClassMember::FunctionDef { .. } | hir::ClassMember::ClassDef { .. } => false,
        _ => false,
    })
}

fn class_members_need_class_cell(members: &[hir::ClassMember]) -> bool {
    members.iter().any(|member| match member {
        hir::ClassMember::FunctionDef {
            uses_zero_argument_super,
            ..
        } => *uses_zero_argument_super,
        hir::ClassMember::If {
            then_body,
            else_body,
            ..
        } => class_members_need_class_cell(then_body) || class_members_need_class_cell(else_body),
        hir::ClassMember::While { body, .. } => class_members_need_class_cell(body),
        hir::ClassMember::For {
            body, else_body, ..
        } => class_members_need_class_cell(body) || class_members_need_class_cell(else_body),
        hir::ClassMember::Try {
            body,
            handlers,
            else_body,
            finally_body,
            ..
        } => {
            class_members_need_class_cell(body)
                || handlers
                    .iter()
                    .any(|handler| class_members_need_class_cell(&handler.body))
                || class_members_need_class_cell(else_body)
                || class_members_need_class_cell(finally_body)
        }
        hir::ClassMember::ClassDef { .. } => false,
        _ => false,
    })
}

impl<'a> Lowerer<'a> {
    fn new(program: &'a mut ProgramLowerer) -> Self {
        Self {
            program,
            blocks: vec![mir::Block {
                parameters: vec![],
                operations: vec![],
                terminator: mir::Terminator::Unreachable,
            }],
            current: mir::BlockId(0),
            next_value: 0,
            cells: BTreeMap::new(),
            exception_edges: BTreeMap::new(),
            exception_target: None,
            cleanups: Vec::new(),
            loops: Vec::new(),
            class_scopes: Vec::new(),
            module_semantics: false,
            qualname_prefix: None,
            nested_uses_locals: false,
        }
    }

    fn child_qualified_name(&self, name: &str) -> String {
        let Some(prefix) = &self.qualname_prefix else {
            return name.to_owned();
        };
        if self.nested_uses_locals {
            format!("{prefix}.<locals>.{name}")
        } else {
            format!("{prefix}.{name}")
        }
    }

    fn statements(&mut self, statements: &[hir::Statement]) -> Result<(), String> {
        for statement in statements {
            if !self.is_open() {
                break;
            }
            self.statement(statement)?;
        }
        Ok(())
    }

    fn install_type_parameters(
        &mut self,
        parameters: &[hir::TypeParameter],
    ) -> Result<InstalledTypeParameters, String> {
        let mut values = Vec::with_capacity(parameters.len());
        let mut previous = Vec::with_capacity(parameters.len());
        for parameter in parameters {
            let value = self.value();
            self.emit(
                parameter.span,
                mir::OperationKind::TypeParameterNew {
                    dest: value,
                    name: parameter.name.clone(),
                    kind: match parameter.kind {
                        hir::TypeParameterKind::TypeVar => mir::TypeParameterKind::TypeVar,
                        hir::TypeParameterKind::TypeVarTuple => {
                            mir::TypeParameterKind::TypeVarTuple
                        }
                        hir::TypeParameterKind::ParamSpec => mir::TypeParameterKind::ParamSpec,
                    },
                },
            );
            let cell = self.value();
            self.emit(
                parameter.span,
                mir::OperationKind::CellNew {
                    dest: cell,
                    initial: Some(value),
                },
            );
            previous.push((
                parameter.name.clone(),
                self.cells.insert(parameter.name.clone(), cell),
            ));
            values.push(value);
        }
        Ok((values, previous))
    }

    fn restore_type_parameters(&mut self, previous: TypeParameterCells) {
        for (name, cell) in previous.into_iter().rev() {
            if let Some(cell) = cell {
                self.cells.insert(name, cell);
            } else {
                self.cells.remove(&name);
            }
        }
    }

    fn type_parameter_tuple(&mut self, span: Span, values: Vec<mir::ValueId>) -> mir::ValueId {
        let tuple = self.value();
        self.emit(
            span,
            mir::OperationKind::Tuple {
                dest: tuple,
                values,
            },
        );
        tuple
    }

    fn statement(&mut self, statement: &hir::Statement) -> Result<(), String> {
        match &statement.kind {
            hir::StatementKind::Assign { targets, value } => {
                // Python evaluates the RHS exactly once before evaluating/storing targets.
                let value = self.expression(value)?;
                for target in targets {
                    self.write_target(target, value)?;
                }
            }
            hir::StatementKind::AugAssign { target, op, value } => {
                self.lower_augmented_target(target, *op, value)?;
            }
            hir::StatementKind::Delete { targets } => {
                for target in targets {
                    self.delete_target(target)?;
                }
            }
            hir::StatementKind::AnnAssign {
                target,
                annotation,
                value,
                simple,
            } => {
                if let Some(value) = value {
                    let value = self.expression(value)?;
                    self.write_target(target, value)?;
                } else {
                    self.evaluate_annotation_target(target)?;
                }
                if self.module_semantics {
                    let annotation = self.expression(annotation)?;
                    if *simple && let hir::TargetKind::Name { name, .. } = &target.kind {
                        self.record_annotation(statement.span, None, name, annotation)?;
                    }
                }
            }
            hir::StatementKind::Assert { test, message } => {
                self.lower_assert(statement.span, test, message.as_ref())?;
            }
            hir::StatementKind::Import { aliases } => {
                for alias in aliases {
                    let value = self.value();
                    self.emit(
                        statement.span,
                        mir::OperationKind::ImportName {
                            dest: value,
                            name: alias.module.clone(),
                        },
                    );
                    self.store_name(statement.span, &alias.bind_name, alias.binding, value)?;
                }
            }
            hir::StatementKind::FunctionDef {
                name,
                binding,
                type_params,
                decorators,
                parameters,
                return_annotation,
                body,
                locals,
                cells,
                free,
            } => {
                let definition_span = decorators
                    .first()
                    .map_or(statement.span, |decorator| decorator.span);
                let decorator_values = decorators
                    .iter()
                    .map(|decorator| self.expression(decorator))
                    .collect::<Result<Vec<_>, _>>()?;
                let qualified_name = self.child_qualified_name(name);
                let function = self.program.lower_scope(
                    name.clone(),
                    qualified_name,
                    parameters,
                    locals,
                    free,
                    body,
                    false,
                )?;
                let mut defaults = Vec::new();
                for (index, parameter) in parameters.iter().enumerate() {
                    if let Some(default) = &parameter.default {
                        defaults.push((
                            u32::try_from(index).map_err(|_| "too many function parameters")?,
                            self.expression(default)?,
                        ));
                    }
                }
                let (type_parameter_values, previous_type_cells) =
                    self.install_type_parameters(type_params)?;
                let annotations = self.lower_function_annotations(
                    statement.span,
                    parameters,
                    return_annotation.as_ref(),
                )?;
                let closure =
                    free.iter()
                        .map(|name| {
                            self.cells.get(name).copied().ok_or_else(|| {
                                format!("free variable `{name}` has no closure cell")
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                let value = self.value();
                self.emit(
                    definition_span,
                    mir::OperationKind::MakeFunction {
                        dest: value,
                        function,
                        defaults,
                        closure,
                        local_names: locals.clone(),
                        cell_names: cells.clone(),
                        free_names: free.clone(),
                    },
                );
                if let Some(annotations) = annotations {
                    self.emit(
                        statement.span,
                        mir::OperationKind::AttributeSet {
                            receiver: value,
                            name: "__annotations__".to_owned(),
                            value: annotations,
                        },
                    );
                }
                if !type_params.is_empty() {
                    let type_params_tuple =
                        self.type_parameter_tuple(statement.span, type_parameter_values);
                    self.emit(
                        statement.span,
                        mir::OperationKind::AttributeSet {
                            receiver: value,
                            name: "__type_params__".to_owned(),
                            value: type_params_tuple,
                        },
                    );
                }
                self.restore_type_parameters(previous_type_cells);
                let mut decorated = value;
                for callable in decorator_values.into_iter().rev() {
                    let next = self.value();
                    self.emit(
                        statement.span,
                        mir::OperationKind::Call {
                            dest: next,
                            callable,
                            positional: vec![decorated],
                            keywords: Vec::new(),
                        },
                    );
                    decorated = next;
                }
                self.store_name(statement.span, name, *binding, decorated)?;
            }
            hir::StatementKind::ClassDef {
                name,
                binding,
                type_params,
                decorators,
                bases,
                metaclass,
                keywords,
                body,
            } => {
                let (type_parameter_values, previous_type_cells) =
                    self.install_type_parameters(type_params)?;
                let bases = bases
                    .iter()
                    .map(|base| self.expression(base))
                    .collect::<Result<Vec<_>, _>>()?;
                let class_keywords = keywords
                    .iter()
                    .map(|(name, value)| Ok((name.clone(), self.expression(value)?)))
                    .collect::<Result<Vec<_>, String>>()?;
                let prepared = if let Some(metaclass) = metaclass {
                    let metaclass = self.expression(metaclass)?;
                    let name_value = self.value();
                    self.emit(
                        statement.span,
                        mir::OperationKind::Constant {
                            dest: name_value,
                            value: mir::Constant::String(name.clone()),
                        },
                    );
                    let bases_value = self.value();
                    self.emit(
                        statement.span,
                        mir::OperationKind::Tuple {
                            dest: bases_value,
                            values: bases.clone(),
                        },
                    );
                    let prepare = self.value();
                    self.emit(
                        statement.span,
                        mir::OperationKind::AttributeGet {
                            dest: prepare,
                            receiver: metaclass,
                            name: "__prepare__".to_owned(),
                        },
                    );
                    let namespace = self.value();
                    self.emit(
                        statement.span,
                        mir::OperationKind::Call {
                            dest: namespace,
                            callable: prepare,
                            positional: vec![name_value, bases_value],
                            keywords: class_keywords.clone(),
                        },
                    );
                    Some((metaclass, name_value, bases_value, namespace))
                } else {
                    None
                };
                let namespace = if let Some((_, _, _, namespace)) = prepared {
                    namespace
                } else {
                    let namespace = self.value();
                    self.emit(
                        statement.span,
                        mir::OperationKind::ClassNamespaceNew { dest: namespace },
                    );
                    namespace
                };
                // CPython seeds these namespace entries before executing the
                // class suite. User code may subsequently overwrite them.
                let module_name = self.value();
                self.emit(
                    statement.span,
                    mir::OperationKind::Constant {
                        dest: module_name,
                        value: mir::Constant::String("__main__".to_owned()),
                    },
                );
                self.emit(
                    statement.span,
                    mir::OperationKind::ClassNamespaceSet {
                        namespace,
                        name: "__module__".to_owned(),
                        value: module_name,
                    },
                );
                let class_qualified_name = self.child_qualified_name(name);
                let qualified_name_value = self.value();
                self.emit(
                    statement.span,
                    mir::OperationKind::Constant {
                        dest: qualified_name_value,
                        value: mir::Constant::String(class_qualified_name.clone()),
                    },
                );
                self.emit(
                    statement.span,
                    mir::OperationKind::ClassNamespaceSet {
                        namespace,
                        name: "__qualname__".to_owned(),
                        value: qualified_name_value,
                    },
                );
                // A class suite is a real native function.  Its namespace is
                // supplied only after `__prepare__`, so every class-local
                // access is scoped to the mapping selected by the metaclass.
                let class_free = self.cells.keys().cloned().collect::<Vec<_>>();
                let class_body = self.program.lower_class_body(
                    format!("<class body {name}>"),
                    format!("{class_qualified_name}.<class body>"),
                    class_qualified_name,
                    body,
                    &class_free,
                )?;
                let class_body_value = self.value();
                let class_closure = class_free
                    .iter()
                    .map(|free_name| {
                        self.cells.get(free_name).copied().ok_or_else(|| {
                            format!("free variable `{free_name}` has no closure cell")
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                self.emit(
                    statement.span,
                    mir::OperationKind::MakeFunction {
                        dest: class_body_value,
                        function: class_body,
                        defaults: Vec::new(),
                        closure: class_closure,
                        local_names: Vec::new(),
                        cell_names: Vec::new(),
                        free_names: class_free.clone(),
                    },
                );
                let class_body_result = self.value();
                self.emit(
                    statement.span,
                    mir::OperationKind::Call {
                        dest: class_body_result,
                        callable: class_body_value,
                        positional: vec![namespace],
                        keywords: Vec::new(),
                    },
                );
                let has_class_cell = class_members_need_class_cell(body);
                let value = if let Some((metaclass, name_value, bases_value, _)) = prepared {
                    let value = self.value();
                    self.emit(
                        statement.span,
                        mir::OperationKind::Call {
                            dest: value,
                            callable: metaclass,
                            positional: vec![name_value, bases_value, namespace],
                            keywords: class_keywords,
                        },
                    );
                    value
                } else {
                    let value = self.value();
                    self.emit(
                        statement.span,
                        mir::OperationKind::ClassNew {
                            dest: value,
                            name: name.clone(),
                            bases,
                            namespace,
                        },
                    );
                    value
                };
                if has_class_cell {
                    let class_cell = self.value();
                    self.emit(
                        statement.span,
                        mir::OperationKind::ClassNamespaceGet {
                            dest: class_cell,
                            namespace,
                            name: "__classcell__".to_owned(),
                        },
                    );
                    self.emit(
                        statement.span,
                        mir::OperationKind::CellSet {
                            cell: class_cell,
                            value,
                        },
                    );
                }
                if !type_params.is_empty() {
                    let type_params_tuple =
                        self.type_parameter_tuple(statement.span, type_parameter_values);
                    self.emit(
                        statement.span,
                        mir::OperationKind::AttributeSet {
                            receiver: value,
                            name: "__type_params__".to_owned(),
                            value: type_params_tuple,
                        },
                    );
                }
                self.restore_type_parameters(previous_type_cells);
                let mut decorated = value;
                for decorator in decorators.iter().rev() {
                    let callable = self.expression(decorator)?;
                    let next = self.value();
                    self.emit(
                        statement.span,
                        mir::OperationKind::Call {
                            dest: next,
                            callable,
                            positional: vec![decorated],
                            keywords: Vec::new(),
                        },
                    );
                    decorated = next;
                }
                self.store_name(statement.span, name, *binding, decorated)?;
            }
            hir::StatementKind::TypeAlias {
                name,
                binding,
                type_params,
                value,
            } => {
                let (type_parameter_values, previous_type_cells) =
                    self.install_type_parameters(type_params)?;
                let alias_value = self.expression(value)?;
                let type_params_tuple =
                    self.type_parameter_tuple(statement.span, type_parameter_values);
                let alias = self.value();
                self.emit(
                    statement.span,
                    mir::OperationKind::TypeAliasNew {
                        dest: alias,
                        name: name.clone(),
                        type_params: type_params_tuple,
                        value: alias_value,
                    },
                );
                self.restore_type_parameters(previous_type_cells);
                self.store_name(statement.span, name, *binding, alias)?;
            }
            hir::StatementKind::Return { value } => {
                let value = value
                    .as_ref()
                    .map(|value| self.expression(value))
                    .transpose()?;
                self.emit_cleanups_from(0)?;
                if self.is_open() {
                    self.terminate(mir::Terminator::ReturnValue { value })?;
                }
            }
            hir::StatementKind::Break => {
                let targets = self
                    .loops
                    .last()
                    .copied()
                    .ok_or_else(|| "break has no enclosing loop".to_owned())?;
                self.emit_cleanups_from(targets.cleanup_depth)?;
                if self.is_open() {
                    self.terminate(mir::Terminator::Jump {
                        target: targets.break_target,
                        arguments: vec![],
                    })?;
                }
            }
            hir::StatementKind::Continue => {
                let targets = self
                    .loops
                    .last()
                    .copied()
                    .ok_or_else(|| "continue has no enclosing loop".to_owned())?;
                self.emit_cleanups_from(targets.cleanup_depth)?;
                if self.is_open() {
                    self.terminate(mir::Terminator::Jump {
                        target: targets.continue_target,
                        arguments: vec![],
                    })?;
                }
            }
            hir::StatementKind::Expression(expression) => {
                self.expression(expression)?;
            }
            hir::StatementKind::Raise { exception, cause } => {
                if let Some(exception) = exception {
                    let exception = self.expression(exception)?;
                    let suppress_context = matches!(
                        cause.as_ref().map(|cause| &cause.kind),
                        Some(hir::ExpressionKind::None)
                    );
                    let cause = cause
                        .as_ref()
                        .map(|cause| self.expression(cause))
                        .transpose()?;
                    self.emit(
                        statement.span,
                        mir::OperationKind::Raise {
                            exception,
                            cause,
                            suppress_context,
                        },
                    );
                } else {
                    self.emit(statement.span, mir::OperationKind::Reraise);
                }
            }
            hir::StatementKind::Try {
                body,
                handlers,
                else_body,
                finally_body,
                is_star,
            } => {
                if *is_star {
                    self.lower_try_star(body, handlers, else_body, finally_body)?;
                } else {
                    self.lower_try(body, handlers, else_body, finally_body)?;
                }
            }
            hir::StatementKind::Print { values } => {
                if let [
                    hir::Expression {
                        kind: hir::ExpressionKind::String(value),
                        ..
                    },
                ] = values.as_slice()
                {
                    self.emit(
                        statement.span,
                        mir::OperationKind::PrintLiteral {
                            value: value.clone(),
                        },
                    );
                } else {
                    let values = values
                        .iter()
                        .map(|value| self.expression(value))
                        .collect::<Result<Vec<_>, _>>()?;
                    self.emit(statement.span, mir::OperationKind::Print { values });
                }
            }
            hir::StatementKind::If {
                condition,
                then_body,
                else_body,
            } => self.lower_if(condition, then_body, else_body)?,
            hir::StatementKind::While { condition, body } => {
                self.lower_while(condition, body)?;
            }
            hir::StatementKind::For {
                target,
                iterable,
                body,
                else_body,
            } => self.lower_for(statement.span, target, iterable, body, else_body)?,
            hir::StatementKind::Match { subject, cases } => {
                self.lower_match(statement.span, subject, cases)?;
            }
        }
        Ok(())
    }

    fn write_target(&mut self, target: &hir::Target, value: mir::ValueId) -> Result<(), String> {
        match &target.kind {
            hir::TargetKind::Name { name, binding } => {
                self.store_name(target.span, name, *binding, value)?;
            }
            hir::TargetKind::Attribute { receiver, name } => {
                let receiver = self.expression(receiver)?;
                self.emit(
                    target.span,
                    mir::OperationKind::AttributeSet {
                        receiver,
                        name: name.clone(),
                        value,
                    },
                );
            }
            hir::TargetKind::Item { collection, index } => {
                let collection = self.expression(collection)?;
                let index = self.expression(index)?;
                self.emit(
                    target.span,
                    mir::OperationKind::ItemSet {
                        collection,
                        index,
                        value,
                    },
                );
            }
            hir::TargetKind::Sequence { elements, .. } => {
                let starred_index = elements
                    .iter()
                    .position(|element| matches!(element.kind, hir::TargetKind::Starred(_)));
                let (before_count, after_count, starred) = match starred_index {
                    Some(index) => (index, elements.len().saturating_sub(index + 1), true),
                    None => (elements.len(), 0, false),
                };
                let unpacked = self.value();
                self.emit(
                    target.span,
                    mir::OperationKind::Unpack {
                        dest: unpacked,
                        value,
                        before_count: u32::try_from(before_count)
                            .map_err(|_| "too many unpacking targets")?,
                        after_count: u32::try_from(after_count)
                            .map_err(|_| "too many unpacking targets")?,
                        starred,
                    },
                );
                for (index, element) in elements.iter().enumerate() {
                    let item = self.value();
                    self.emit(
                        element.span,
                        mir::OperationKind::ValueArrayGet {
                            dest: item,
                            array: unpacked,
                            index: u32::try_from(index)
                                .map_err(|_| "too many unpacking targets")?,
                        },
                    );
                    match &element.kind {
                        hir::TargetKind::Starred(inner) => self.write_target(inner, item)?,
                        _ => self.write_target(element, item)?,
                    }
                }
            }
            hir::TargetKind::Starred(inner) => self.write_target(inner, value)?,
        }
        Ok(())
    }

    fn write_clause_target(
        &mut self,
        target: &hir::Target,
        value: mir::ValueId,
    ) -> Result<(), String> {
        self.write_target(target, value)
    }

    fn write_class_target(
        &mut self,
        namespace: mir::ValueId,
        target: &hir::Target,
        value: mir::ValueId,
    ) -> Result<(), String> {
        match &target.kind {
            hir::TargetKind::Name { name, binding } => match binding {
                hir::Binding::Global => self.emit(
                    target.span,
                    mir::OperationKind::GlobalSet {
                        name: name.clone(),
                        value,
                    },
                ),
                hir::Binding::Free | hir::Binding::Cell | hir::Binding::Local => {
                    let cell = self
                        .cells
                        .get(name)
                        .copied()
                        .ok_or_else(|| format!("class target `{name}` has no cell"))?;
                    self.emit(target.span, mir::OperationKind::CellSet { cell, value });
                }
                hir::Binding::ClassName | hir::Binding::ClassFree => {
                    self.emit(
                        target.span,
                        mir::OperationKind::ClassNamespaceSet {
                            namespace,
                            name: name.clone(),
                            value,
                        },
                    );
                    self.record_class_name(name);
                }
            },
            hir::TargetKind::Attribute { receiver, name } => {
                let receiver = self.expression(receiver)?;
                self.emit(
                    target.span,
                    mir::OperationKind::AttributeSet {
                        receiver,
                        name: name.clone(),
                        value,
                    },
                );
            }
            hir::TargetKind::Item { collection, index } => {
                let collection = self.expression(collection)?;
                let index = self.expression(index)?;
                self.emit(
                    target.span,
                    mir::OperationKind::ItemSet {
                        collection,
                        index,
                        value,
                    },
                );
            }
            hir::TargetKind::Sequence { elements, .. } => {
                let starred_index = elements
                    .iter()
                    .position(|element| matches!(element.kind, hir::TargetKind::Starred(_)));
                let (before_count, after_count, starred) = match starred_index {
                    Some(index) => (index, elements.len().saturating_sub(index + 1), true),
                    None => (elements.len(), 0, false),
                };
                let unpacked = self.value();
                self.emit(
                    target.span,
                    mir::OperationKind::Unpack {
                        dest: unpacked,
                        value,
                        before_count: u32::try_from(before_count)
                            .map_err(|_| "too many loop-target elements")?,
                        after_count: u32::try_from(after_count)
                            .map_err(|_| "too many loop-target elements")?,
                        starred,
                    },
                );
                for (index, element) in elements.iter().enumerate() {
                    let item = self.value();
                    self.emit(
                        element.span,
                        mir::OperationKind::ValueArrayGet {
                            dest: item,
                            array: unpacked,
                            index: u32::try_from(index)
                                .map_err(|_| "too many loop-target elements")?,
                        },
                    );
                    let element = match &element.kind {
                        hir::TargetKind::Starred(inner) => inner.as_ref(),
                        _ => element,
                    };
                    self.write_class_target(namespace, element, item)?;
                }
            }
            hir::TargetKind::Starred(inner) => {
                self.write_class_target(namespace, inner, value)?;
            }
        }
        Ok(())
    }

    fn delete_target(&mut self, target: &hir::Target) -> Result<(), String> {
        match &target.kind {
            hir::TargetKind::Name { name, binding } => {
                self.clear_name(target.span, name, *binding)?;
            }
            hir::TargetKind::Attribute { receiver, name } => {
                let receiver = self.expression(receiver)?;
                self.emit(
                    target.span,
                    mir::OperationKind::AttributeDelete {
                        receiver,
                        name: name.clone(),
                    },
                );
            }
            hir::TargetKind::Item { collection, index } => {
                let collection = self.expression(collection)?;
                let index = self.expression(index)?;
                self.emit(
                    target.span,
                    mir::OperationKind::ItemDelete { collection, index },
                );
            }
            hir::TargetKind::Sequence { elements, .. } => {
                for element in elements {
                    self.delete_target(element)?;
                }
            }
            hir::TargetKind::Starred(_) => {
                return Err("starred deletion target reached MIR".to_owned());
            }
        }
        Ok(())
    }

    fn delete_class_target(
        &mut self,
        namespace: mir::ValueId,
        target: &hir::Target,
    ) -> Result<(), String> {
        match &target.kind {
            hir::TargetKind::Name { name, binding } => match binding {
                hir::Binding::Global => self.emit(
                    target.span,
                    mir::OperationKind::GlobalDelete { name: name.clone() },
                ),
                hir::Binding::Free | hir::Binding::Cell | hir::Binding::Local => {
                    let cell = self
                        .cells
                        .get(name)
                        .copied()
                        .ok_or_else(|| format!("class delete target `{name}` has no cell"))?;
                    self.emit(target.span, mir::OperationKind::CellClear { cell });
                }
                hir::Binding::ClassName | hir::Binding::ClassFree => self.emit(
                    target.span,
                    mir::OperationKind::ClassNamespaceDelete {
                        namespace,
                        name: name.clone(),
                    },
                ),
            },
            hir::TargetKind::Attribute { receiver, name } => {
                let receiver = self.expression(receiver)?;
                self.emit(
                    target.span,
                    mir::OperationKind::AttributeDelete {
                        receiver,
                        name: name.clone(),
                    },
                );
            }
            hir::TargetKind::Item { collection, index } => {
                let collection = self.expression(collection)?;
                let index = self.expression(index)?;
                self.emit(
                    target.span,
                    mir::OperationKind::ItemDelete { collection, index },
                );
            }
            hir::TargetKind::Sequence { elements, .. } => {
                for element in elements {
                    self.delete_class_target(namespace, element)?;
                }
            }
            hir::TargetKind::Starred(_) => {
                return Err("starred class deletion target reached MIR".to_owned());
            }
        }
        Ok(())
    }

    fn lower_augmented_target(
        &mut self,
        target: &hir::Target,
        op: hir::BinaryOperator,
        value: &hir::Expression,
    ) -> Result<(), String> {
        match &target.kind {
            hir::TargetKind::Name { name, binding } => {
                let current = self.expression(&hir::Expression {
                    span: target.span,
                    kind: hir::ExpressionKind::Name {
                        name: name.clone(),
                        binding: *binding,
                    },
                })?;
                let value = self.expression(value)?;
                let result = self.value();
                self.emit(
                    target.span,
                    mir::OperationKind::InPlace {
                        dest: result,
                        op,
                        left: current,
                        right: value,
                    },
                );
                self.store_name(target.span, name, *binding, result)?;
            }
            hir::TargetKind::Attribute { receiver, name } => {
                let receiver = self.expression(receiver)?;
                let current = self.value();
                self.emit(
                    target.span,
                    mir::OperationKind::AttributeGet {
                        dest: current,
                        receiver,
                        name: name.clone(),
                    },
                );
                let value = self.expression(value)?;
                let result = self.value();
                self.emit(
                    target.span,
                    mir::OperationKind::InPlace {
                        dest: result,
                        op,
                        left: current,
                        right: value,
                    },
                );
                self.emit(
                    target.span,
                    mir::OperationKind::AttributeSet {
                        receiver,
                        name: name.clone(),
                        value: result,
                    },
                );
            }
            hir::TargetKind::Item { collection, index } => {
                let collection = self.expression(collection)?;
                let index = self.expression(index)?;
                let current = self.value();
                self.emit(
                    target.span,
                    mir::OperationKind::ItemGet {
                        dest: current,
                        collection,
                        index,
                    },
                );
                let value = self.expression(value)?;
                let result = self.value();
                self.emit(
                    target.span,
                    mir::OperationKind::InPlace {
                        dest: result,
                        op,
                        left: current,
                        right: value,
                    },
                );
                self.emit(
                    target.span,
                    mir::OperationKind::ItemSet {
                        collection,
                        index,
                        value: result,
                    },
                );
            }
            hir::TargetKind::Sequence { .. } | hir::TargetKind::Starred(_) => {
                return Err(
                    "invalid sequence augmented-assignment target reached MIR after semantic validation"
                        .to_owned(),
                );
            }
        }
        Ok(())
    }

    fn lower_class_augmented_target(
        &mut self,
        span: Span,
        namespace: mir::ValueId,
        target: &hir::Target,
        op: hir::BinaryOperator,
        value: &hir::Expression,
    ) -> Result<(), String> {
        match &target.kind {
            hir::TargetKind::Name { name, binding } => {
                let current = self.value();
                let load = match binding {
                    hir::Binding::Global => mir::OperationKind::GlobalGet {
                        dest: current,
                        name: name.clone(),
                    },
                    hir::Binding::Free | hir::Binding::Cell | hir::Binding::Local => {
                        let cell = self.cells.get(name).copied().ok_or_else(|| {
                            format!("class augmented target `{name}` has no cell")
                        })?;
                        mir::OperationKind::CellGet {
                            dest: current,
                            cell,
                            name: Some(name.clone()),
                            free: *binding == hir::Binding::Free,
                        }
                    }
                    hir::Binding::ClassName => mir::OperationKind::ClassNameGet {
                        dest: current,
                        namespace,
                        name: name.clone(),
                    },
                    hir::Binding::ClassFree => {
                        let cell = self.cells.get(name).copied().ok_or_else(|| {
                            format!("class free augmented target `{name}` has no cell")
                        })?;
                        mir::OperationKind::ClassFreeGet {
                            dest: current,
                            namespace,
                            cell,
                            name: name.clone(),
                        }
                    }
                };
                self.emit(target.span, load);
                let right = self.expression(value)?;
                let result = self.value();
                self.emit(
                    span,
                    mir::OperationKind::InPlace {
                        dest: result,
                        op,
                        left: current,
                        right,
                    },
                );
                self.write_class_target(namespace, target, result)?;
            }
            hir::TargetKind::Attribute { receiver, name } => {
                let receiver = self.expression(receiver)?;
                let current = self.value();
                self.emit(
                    target.span,
                    mir::OperationKind::AttributeGet {
                        dest: current,
                        receiver,
                        name: name.clone(),
                    },
                );
                let right = self.expression(value)?;
                let result = self.value();
                self.emit(
                    span,
                    mir::OperationKind::InPlace {
                        dest: result,
                        op,
                        left: current,
                        right,
                    },
                );
                self.emit(
                    target.span,
                    mir::OperationKind::AttributeSet {
                        receiver,
                        name: name.clone(),
                        value: result,
                    },
                );
            }
            hir::TargetKind::Item { collection, index } => {
                let collection = self.expression(collection)?;
                let index = self.expression(index)?;
                let current = self.value();
                self.emit(
                    target.span,
                    mir::OperationKind::ItemGet {
                        dest: current,
                        collection,
                        index,
                    },
                );
                let right = self.expression(value)?;
                let result = self.value();
                self.emit(
                    span,
                    mir::OperationKind::InPlace {
                        dest: result,
                        op,
                        left: current,
                        right,
                    },
                );
                self.emit(
                    target.span,
                    mir::OperationKind::ItemSet {
                        collection,
                        index,
                        value: result,
                    },
                );
            }
            hir::TargetKind::Sequence { .. } | hir::TargetKind::Starred(_) => {
                return Err("invalid augmented class target reached MIR".to_owned());
            }
        }
        Ok(())
    }

    fn lower_assert(
        &mut self,
        span: Span,
        test: &hir::Expression,
        message: Option<&hir::Expression>,
    ) -> Result<(), String> {
        let condition = self.expression(test)?;
        let passed = self.new_block();
        let failed = self.new_block();
        self.terminate(mir::Terminator::Branch {
            condition,
            then_target: passed,
            then_arguments: vec![],
            else_target: failed,
            else_arguments: vec![],
        })?;
        self.current = failed;
        let assertion_error = self.value();
        self.emit(
            span,
            mir::OperationKind::GlobalGet {
                dest: assertion_error,
                name: "AssertionError".to_owned(),
            },
        );
        let positional = message
            .map(|message| self.expression(message).map(|value| vec![value]))
            .transpose()?
            .unwrap_or_default();
        let exception = self.value();
        self.emit(
            span,
            mir::OperationKind::Call {
                dest: exception,
                callable: assertion_error,
                positional,
                keywords: Vec::new(),
            },
        );
        self.emit(
            span,
            mir::OperationKind::Raise {
                exception,
                cause: None,
                suppress_context: false,
            },
        );
        if self.is_open() {
            self.terminate(mir::Terminator::Jump {
                target: passed,
                arguments: vec![],
            })?;
        }
        self.current = passed;
        Ok(())
    }

    fn store_name(
        &mut self,
        span: Span,
        name: &str,
        binding: hir::Binding,
        value: mir::ValueId,
    ) -> Result<(), String> {
        match binding {
            hir::Binding::Global => self.emit(
                span,
                mir::OperationKind::GlobalSet {
                    name: name.to_owned(),
                    value,
                },
            ),
            hir::Binding::Local | hir::Binding::Cell | hir::Binding::Free => {
                let cell = self
                    .cells
                    .get(name)
                    .copied()
                    .ok_or_else(|| format!("local `{name}` has no cell"))?;
                self.emit(span, mir::OperationKind::CellSet { cell, value });
            }
            hir::Binding::ClassName | hir::Binding::ClassFree => {
                let namespace = self
                    .class_scopes
                    .last()
                    .map(|scope| scope.namespace)
                    .ok_or_else(|| format!("class binding `{name}` escaped class lowering"))?;
                self.emit(
                    span,
                    mir::OperationKind::ClassNamespaceSet {
                        namespace,
                        name: name.to_owned(),
                        value,
                    },
                );
                self.record_class_name(name);
            }
        }
        Ok(())
    }

    fn evaluate_annotation_target(&mut self, target: &hir::Target) -> Result<(), String> {
        match &target.kind {
            hir::TargetKind::Name { .. } => {}
            hir::TargetKind::Attribute { receiver, .. } => {
                self.expression(receiver)?;
            }
            hir::TargetKind::Item { collection, index } => {
                self.expression(collection)?;
                self.expression(index)?;
            }
            hir::TargetKind::Sequence { .. } | hir::TargetKind::Starred(_) => {
                return Err("invalid annotated-assignment target reached MIR".to_owned());
            }
        }
        Ok(())
    }

    fn record_annotation(
        &mut self,
        span: Span,
        namespace: Option<mir::ValueId>,
        name: &str,
        annotation: mir::ValueId,
    ) -> Result<(), String> {
        let mapping = self.value();
        if let Some(namespace) = namespace {
            self.emit(
                span,
                mir::OperationKind::ClassNamespaceGet {
                    dest: mapping,
                    namespace,
                    name: "__annotations__".to_owned(),
                },
            );
        } else {
            self.emit(
                span,
                mir::OperationKind::GlobalGet {
                    dest: mapping,
                    name: "__annotations__".to_owned(),
                },
            );
        }
        let key = self.value();
        self.emit(
            span,
            mir::OperationKind::Constant {
                dest: key,
                value: mir::Constant::String(name.to_owned()),
            },
        );
        self.emit(
            span,
            mir::OperationKind::ItemSet {
                collection: mapping,
                index: key,
                value: annotation,
            },
        );
        Ok(())
    }

    fn lower_function_annotations(
        &mut self,
        span: Span,
        parameters: &[hir::Parameter],
        return_annotation: Option<&hir::Expression>,
    ) -> Result<Option<mir::ValueId>, String> {
        let mut keys = Vec::new();
        let mut values = Vec::new();
        for parameter in parameters {
            if let Some(annotation) = &parameter.annotation {
                let value = self.expression(annotation)?;
                let key = self.value();
                self.emit(
                    span,
                    mir::OperationKind::Constant {
                        dest: key,
                        value: mir::Constant::String(parameter.name.clone()),
                    },
                );
                keys.push(key);
                values.push(value);
            }
        }
        if let Some(annotation) = return_annotation {
            let value = self.expression(annotation)?;
            let key = self.value();
            self.emit(
                span,
                mir::OperationKind::Constant {
                    dest: key,
                    value: mir::Constant::String("return".to_owned()),
                },
            );
            keys.push(key);
            values.push(value);
        }
        if keys.is_empty() {
            return Ok(None);
        }
        let annotations = self.value();
        self.emit(
            span,
            mir::OperationKind::Dictionary {
                dest: annotations,
                keys,
                values,
            },
        );
        Ok(Some(annotations))
    }

    fn lower_class_conditional_assignments(
        &mut self,
        span: Span,
        namespace: mir::ValueId,
        condition: &hir::Expression,
        then_body: &[hir::ClassMember],
        else_body: &[hir::ClassMember],
    ) -> Result<(), String> {
        let condition = self.expression(condition)?;
        let then_block = self.new_block();
        let else_block = self.new_block();
        let join = self.new_block();
        self.terminate(mir::Terminator::Branch {
            condition,
            then_target: then_block,
            then_arguments: vec![],
            else_target: else_block,
            else_arguments: vec![],
        })?;
        self.current = then_block;
        self.lower_class_assignment_members(span, namespace, then_body)?;
        if self.is_open() {
            self.terminate(mir::Terminator::Jump {
                target: join,
                arguments: vec![],
            })?;
        }
        self.current = else_block;
        self.lower_class_assignment_members(span, namespace, else_body)?;
        if self.is_open() {
            self.terminate(mir::Terminator::Jump {
                target: join,
                arguments: vec![],
            })?;
        }
        self.current = join;
        Ok(())
    }

    fn lower_class_assignment_members(
        &mut self,
        span: Span,
        namespace: mir::ValueId,
        members: &[hir::ClassMember],
    ) -> Result<(), String> {
        for member in members {
            match member {
                hir::ClassMember::Assign { targets, value } => {
                    let value = self.expression(value)?;
                    for target in targets {
                        self.write_class_target(namespace, target, value)?;
                    }
                }
                hir::ClassMember::AugAssign { target, op, value } => {
                    self.lower_class_augmented_target(span, namespace, target, *op, value)?;
                }
                hir::ClassMember::Delete { targets } => {
                    for target in targets {
                        self.delete_class_target(namespace, target)?;
                    }
                }
                hir::ClassMember::AnnAssign {
                    target,
                    annotation,
                    value,
                    simple,
                } => {
                    if let Some(value) = value {
                        let value = self.expression(value)?;
                        self.write_class_target(namespace, target, value)?;
                    } else {
                        self.evaluate_annotation_target(target)?;
                    }
                    let annotation = self.expression(annotation)?;
                    if *simple && let hir::TargetKind::Name { name, .. } = &target.kind {
                        self.record_annotation(span, Some(namespace), name, annotation)?;
                    }
                }
                hir::ClassMember::Assert { test, message } => {
                    self.lower_assert(span, test, message.as_ref())?;
                }
                hir::ClassMember::Raise { exception, cause } => {
                    if let Some(exception) = exception {
                        let exception = self.expression(exception)?;
                        let suppress_context = matches!(
                            cause.as_ref().map(|cause| &cause.kind),
                            Some(hir::ExpressionKind::None)
                        );
                        let cause = cause
                            .as_ref()
                            .map(|value| self.expression(value))
                            .transpose()?;
                        self.emit(
                            span,
                            mir::OperationKind::Raise {
                                exception,
                                cause,
                                suppress_context,
                            },
                        );
                    } else {
                        self.emit(span, mir::OperationKind::Reraise);
                    }
                }
                hir::ClassMember::Try {
                    body,
                    handlers,
                    else_body,
                    finally_body,
                    is_star,
                } => self.lower_class_try_members(
                    span,
                    namespace,
                    body,
                    handlers,
                    else_body,
                    finally_body,
                    *is_star,
                )?,
                hir::ClassMember::Import { aliases } => {
                    for alias in aliases {
                        let value = self.value();
                        self.emit(
                            span,
                            mir::OperationKind::ImportName {
                                dest: value,
                                name: alias.module.clone(),
                            },
                        );
                        self.store_name(span, &alias.bind_name, alias.binding, value)?;
                    }
                }
                hir::ClassMember::ClassDef {
                    name,
                    binding,
                    type_params,
                    decorators,
                    bases,
                    metaclass,
                    keywords,
                    body,
                } => self.lower_nested_class_member(
                    span,
                    namespace,
                    name,
                    *binding,
                    type_params,
                    decorators,
                    bases,
                    metaclass,
                    keywords,
                    body,
                )?,
                hir::ClassMember::TypeAlias {
                    name,
                    binding,
                    type_params,
                    value,
                } => {
                    let (type_parameter_values, previous_type_cells) =
                        self.install_type_parameters(type_params)?;
                    let alias_value = self.expression(value)?;
                    let type_params_tuple = self.type_parameter_tuple(span, type_parameter_values);
                    let alias = self.value();
                    self.emit(
                        span,
                        mir::OperationKind::TypeAliasNew {
                            dest: alias,
                            name: name.clone(),
                            type_params: type_params_tuple,
                            value: alias_value,
                        },
                    );
                    self.restore_type_parameters(previous_type_cells);
                    self.store_name(span, name, *binding, alias)?;
                }
                hir::ClassMember::Break => {
                    let targets = self
                        .loops
                        .last()
                        .copied()
                        .ok_or_else(|| "break has no enclosing loop".to_owned())?;
                    self.emit_cleanups_from(targets.cleanup_depth)?;
                    self.terminate(mir::Terminator::Jump {
                        target: targets.break_target,
                        arguments: vec![],
                    })?;
                }
                hir::ClassMember::Continue => {
                    let targets = self
                        .loops
                        .last()
                        .copied()
                        .ok_or_else(|| "continue has no enclosing loop".to_owned())?;
                    self.emit_cleanups_from(targets.cleanup_depth)?;
                    self.terminate(mir::Terminator::Jump {
                        target: targets.continue_target,
                        arguments: vec![],
                    })?;
                }
                hir::ClassMember::If {
                    condition,
                    then_body,
                    else_body,
                } => self.lower_class_conditional_assignments(
                    span, namespace, condition, then_body, else_body,
                )?,
                hir::ClassMember::While { condition, body } => {
                    self.lower_class_while_assignments(span, namespace, condition, body)?
                }
                hir::ClassMember::For {
                    target,
                    iterable,
                    body,
                    else_body,
                } => self
                    .lower_class_for_members(span, namespace, target, iterable, body, else_body)?,
                hir::ClassMember::Expression(expression) => {
                    self.expression(expression)?;
                }
                hir::ClassMember::Print(values) => {
                    let values = values
                        .iter()
                        .map(|value| self.expression(value))
                        .collect::<Result<Vec<_>, _>>()?;
                    self.emit(span, mir::OperationKind::Print { values });
                }
                hir::ClassMember::FunctionDef {
                    span: member_span,
                    name,
                    binding,
                    type_params,
                    decorators,
                    uses_zero_argument_super,
                    parameters,
                    return_annotation,
                    body,
                    locals,
                    cells,
                    free,
                } => {
                    let definition_span = decorators
                        .first()
                        .map_or(*member_span, |decorator| decorator.span);
                    let mut method_free = free.clone();
                    if *uses_zero_argument_super
                        && !method_free.iter().any(|name| name == "__class__")
                    {
                        method_free.push("__class__".to_owned());
                    }
                    let decorator_values = decorators
                        .iter()
                        .map(|decorator| self.class_decorator_expression(decorator, namespace))
                        .collect::<Result<Vec<_>, _>>()?;
                    let function = self.program.lower_scope(
                        name.clone(),
                        self.child_qualified_name(name),
                        parameters,
                        locals,
                        &method_free,
                        body,
                        false,
                    )?;
                    let mut defaults = Vec::new();
                    for (index, parameter) in parameters.iter().enumerate() {
                        if let Some(default) = &parameter.default {
                            defaults.push((
                                u32::try_from(index).map_err(|_| "too many method parameters")?,
                                self.expression(default)?,
                            ));
                        }
                    }
                    let (type_parameter_values, previous_type_cells) =
                        self.install_type_parameters(type_params)?;
                    let annotations = self.lower_function_annotations(
                        *member_span,
                        parameters,
                        return_annotation.as_ref(),
                    )?;
                    let closure = method_free
                        .iter()
                        .map(|free_name| {
                            self.cells.get(free_name).copied().ok_or_else(|| {
                                format!("free variable `{free_name}` has no closure cell")
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let method = self.value();
                    self.emit(
                        definition_span,
                        mir::OperationKind::MakeFunction {
                            dest: method,
                            function,
                            defaults,
                            closure,
                            local_names: locals.clone(),
                            cell_names: cells.clone(),
                            free_names: method_free.clone(),
                        },
                    );
                    if let Some(annotations) = annotations {
                        self.emit(
                            span,
                            mir::OperationKind::AttributeSet {
                                receiver: method,
                                name: "__annotations__".to_owned(),
                                value: annotations,
                            },
                        );
                    }
                    if !type_params.is_empty() {
                        let type_params_tuple =
                            self.type_parameter_tuple(span, type_parameter_values);
                        self.emit(
                            span,
                            mir::OperationKind::AttributeSet {
                                receiver: method,
                                name: "__type_params__".to_owned(),
                                value: type_params_tuple,
                            },
                        );
                    }
                    self.restore_type_parameters(previous_type_cells);
                    let mut decorated = method;
                    for callable in decorator_values.into_iter().rev() {
                        let value = self.value();
                        self.emit(
                            span,
                            mir::OperationKind::Call {
                                dest: value,
                                callable,
                                positional: vec![decorated],
                                keywords: Vec::new(),
                            },
                        );
                        decorated = value;
                    }
                    self.store_name(span, name, *binding, decorated)?;
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_nested_class_member(
        &mut self,
        span: Span,
        namespace: mir::ValueId,
        name: &str,
        binding: hir::Binding,
        type_params: &[hir::TypeParameter],
        decorators: &[hir::Expression],
        bases: &[hir::Expression],
        metaclass: &Option<hir::Expression>,
        keywords: &[(String, hir::Expression)],
        body: &[hir::ClassMember],
    ) -> Result<(), String> {
        let statement = hir::Statement {
            span,
            kind: hir::StatementKind::ClassDef {
                name: name.to_owned(),
                binding,
                type_params: type_params.to_vec(),
                decorators: decorators.to_vec(),
                bases: bases.to_vec(),
                metaclass: metaclass.clone(),
                keywords: keywords.to_vec(),
                body: body.to_vec(),
            },
        };
        let _ = namespace;
        self.statement(&statement)
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_class_try_members(
        &mut self,
        span: Span,
        namespace: mir::ValueId,
        body: &[hir::ClassMember],
        handlers: &[hir::ClassExceptionHandler],
        else_body: &[hir::ClassMember],
        finally_body: &[hir::ClassMember],
        is_star: bool,
    ) -> Result<(), String> {
        if is_star {
            return Err(
                "except-star in a class body requires native class-body functions".to_owned(),
            );
        }
        let outer_exception = self.exception_target;
        let join = self.new_block();
        let normal_target = if finally_body.is_empty() {
            join
        } else {
            self.new_block()
        };
        let finally_exception = (!finally_body.is_empty()).then(|| self.new_block());
        let inner_exception = finally_exception.or(outer_exception);
        let propagation_target = finally_exception.unwrap_or(join);
        let finally_cleanup = (!finally_body.is_empty()).then(|| CleanupAction::ClassFinally {
            namespace,
            members: finally_body.to_vec(),
            exception_target: outer_exception,
        });
        if let Some(cleanup) = &finally_cleanup {
            self.cleanups.push(cleanup.clone());
        }
        let dispatch = (!handlers.is_empty()).then(|| self.new_block());
        self.exception_target = dispatch.or(inner_exception);
        self.lower_class_assignment_members(span, namespace, body)?;
        if self.is_open() {
            let else_block = self.new_block();
            self.terminate(mir::Terminator::Jump {
                target: else_block,
                arguments: vec![],
            })?;
            self.current = else_block;
            self.exception_target = inner_exception;
            self.lower_class_assignment_members(span, namespace, else_body)?;
            if self.is_open() {
                self.terminate(mir::Terminator::Jump {
                    target: normal_target,
                    arguments: vec![],
                })?;
            }
        }

        if let Some(dispatch) = dispatch {
            self.current = dispatch;
            self.exception_target = inner_exception;
            let active = self.value();
            self.emit(span, mir::OperationKind::ExceptionActive { dest: active });
            let mut next_test = dispatch;
            for (index, handler) in handlers.iter().enumerate() {
                if index != 0 {
                    self.current = next_test;
                }
                let selected = self.new_block();
                if let Some(expected) = &handler.exception_type {
                    let expected = self.expression(expected)?;
                    let matches = self.value();
                    self.emit(
                        span,
                        mir::OperationKind::ExceptionMatches {
                            dest: matches,
                            exception: active,
                            expected_type: expected,
                        },
                    );
                    next_test = self.new_block();
                    self.terminate(mir::Terminator::Branch {
                        condition: matches,
                        then_target: selected,
                        then_arguments: vec![],
                        else_target: next_test,
                        else_arguments: vec![],
                    })?;
                } else {
                    self.terminate(mir::Terminator::Jump {
                        target: selected,
                        arguments: vec![],
                    })?;
                    next_test = self.new_block();
                }

                self.current = selected;
                let handled = self.value();
                self.emit(span, mir::OperationKind::HandlerEnter { dest: handled });
                if let Some((name, binding)) = &handler.name {
                    self.store_name(span, name, *binding, handled)?;
                }
                let cleanup_error = self.new_block();
                self.exception_target = Some(cleanup_error);
                self.cleanups.push(CleanupAction::ClassHandler {
                    binding: handler.name.clone(),
                    exception_target: inner_exception,
                });
                self.lower_class_assignment_members(span, namespace, &handler.body)?;
                self.cleanups.pop();
                if self.is_open() {
                    if let Some((name, binding)) = &handler.name {
                        self.clear_name(span, name, *binding)?;
                    }
                    self.exception_target = inner_exception;
                    self.emit(span, mir::OperationKind::HandlerLeave);
                    self.terminate(mir::Terminator::Jump {
                        target: normal_target,
                        arguments: vec![],
                    })?;
                }
                self.current = cleanup_error;
                self.exception_target = inner_exception;
                if let Some((name, binding)) = &handler.name {
                    self.clear_name(span, name, *binding)?;
                }
                self.emit(span, mir::OperationKind::HandlerLeave);
                self.emit(span, mir::OperationKind::Propagate);
                self.terminate(mir::Terminator::Jump {
                    target: propagation_target,
                    arguments: vec![],
                })?;
            }
            self.current = next_test;
            self.exception_target = inner_exception;
            self.emit(span, mir::OperationKind::Propagate);
            self.terminate(mir::Terminator::Jump {
                target: propagation_target,
                arguments: vec![],
            })?;
        }

        if finally_cleanup.is_some() {
            self.cleanups.pop();
        }
        if let Some(finally_exception) = finally_exception {
            self.current = finally_exception;
            self.exception_target = outer_exception;
            self.lower_class_assignment_members(span, namespace, finally_body)?;
            if self.is_open() {
                self.emit(span, mir::OperationKind::Propagate);
                self.terminate(mir::Terminator::Jump {
                    target: join,
                    arguments: vec![],
                })?;
            }
            self.current = normal_target;
            self.exception_target = outer_exception;
            self.lower_class_assignment_members(span, namespace, finally_body)?;
            if self.is_open() {
                self.terminate(mir::Terminator::Jump {
                    target: join,
                    arguments: vec![],
                })?;
            }
        }
        self.current = join;
        self.exception_target = outer_exception;
        Ok(())
    }

    fn lower_class_while_assignments(
        &mut self,
        span: Span,
        namespace: mir::ValueId,
        condition: &hir::Expression,
        body: &[hir::ClassMember],
    ) -> Result<(), String> {
        let header = self.new_block();
        let body_block = self.new_block();
        let exit = self.new_block();
        self.terminate(mir::Terminator::Jump {
            target: header,
            arguments: vec![],
        })?;
        self.current = header;
        let condition = self.expression(condition)?;
        self.terminate(mir::Terminator::Branch {
            condition,
            then_target: body_block,
            then_arguments: vec![],
            else_target: exit,
            else_arguments: vec![],
        })?;
        self.current = body_block;
        self.loops.push(LoopTargets {
            continue_target: header,
            break_target: exit,
            cleanup_depth: self.cleanups.len(),
        });
        self.lower_class_assignment_members(span, namespace, body)?;
        self.loops.pop();
        if self.is_open() {
            self.terminate(mir::Terminator::Jump {
                target: header,
                arguments: vec![],
            })?;
        }
        self.current = exit;
        Ok(())
    }

    fn lower_class_for_members(
        &mut self,
        span: Span,
        namespace: mir::ValueId,
        target: &hir::Target,
        iterable: &hir::Expression,
        body: &[hir::ClassMember],
        else_body: &[hir::ClassMember],
    ) -> Result<(), String> {
        let iterable = self.expression(iterable)?;
        let iterator = self.value();
        self.emit(
            span,
            mir::OperationKind::IteratorNew {
                dest: iterator,
                value: iterable,
            },
        );
        let header = self.new_block();
        let body_block = self.new_block();
        let exhausted = self.new_block();
        let exit = self.new_block();
        self.terminate(mir::Terminator::Jump {
            target: header,
            arguments: vec![],
        })?;
        self.current = header;
        let item = self.value();
        let has_value = self.value();
        self.emit(
            span,
            mir::OperationKind::IteratorNext {
                item,
                has_value,
                iterator,
            },
        );
        self.terminate(mir::Terminator::Branch {
            condition: has_value,
            then_target: body_block,
            then_arguments: vec![],
            else_target: exhausted,
            else_arguments: vec![],
        })?;
        self.current = body_block;
        self.write_class_target(namespace, target, item)?;
        self.loops.push(LoopTargets {
            continue_target: header,
            break_target: exit,
            cleanup_depth: self.cleanups.len(),
        });
        self.lower_class_assignment_members(span, namespace, body)?;
        self.loops.pop();
        if self.is_open() {
            self.terminate(mir::Terminator::Jump {
                target: header,
                arguments: vec![],
            })?;
        }
        self.current = exhausted;
        self.lower_class_assignment_members(span, namespace, else_body)?;
        if self.is_open() {
            self.terminate(mir::Terminator::Jump {
                target: exit,
                arguments: vec![],
            })?;
        }
        self.current = exit;
        Ok(())
    }

    fn record_class_name(&mut self, _name: &str) {}

    fn lower_if(
        &mut self,
        condition: &hir::Expression,
        then_body: &[hir::Statement],
        else_body: &[hir::Statement],
    ) -> Result<(), String> {
        let condition = self.expression(condition)?;
        let then_block = self.new_block();
        let else_block = self.new_block();
        let join = self.new_block();
        self.terminate(mir::Terminator::Branch {
            condition,
            then_target: then_block,
            then_arguments: vec![],
            else_target: else_block,
            else_arguments: vec![],
        })?;
        self.current = then_block;
        self.statements(then_body)?;
        if self.is_open() {
            self.terminate(mir::Terminator::Jump {
                target: join,
                arguments: vec![],
            })?;
        }
        self.current = else_block;
        self.statements(else_body)?;
        if self.is_open() {
            self.terminate(mir::Terminator::Jump {
                target: join,
                arguments: vec![],
            })?;
        }
        self.current = join;
        Ok(())
    }

    fn lower_while(
        &mut self,
        condition: &hir::Expression,
        body: &[hir::Statement],
    ) -> Result<(), String> {
        let header = self.new_block();
        let body_block = self.new_block();
        let exit = self.new_block();
        self.terminate(mir::Terminator::Jump {
            target: header,
            arguments: vec![],
        })?;
        self.current = header;
        let condition = self.expression(condition)?;
        self.terminate(mir::Terminator::Branch {
            condition,
            then_target: body_block,
            then_arguments: vec![],
            else_target: exit,
            else_arguments: vec![],
        })?;
        self.current = body_block;
        self.loops.push(LoopTargets {
            continue_target: header,
            break_target: exit,
            cleanup_depth: self.cleanups.len(),
        });
        self.statements(body)?;
        self.loops.pop();
        if self.is_open() {
            self.terminate(mir::Terminator::Jump {
                target: header,
                arguments: vec![],
            })?;
        }
        self.current = exit;
        Ok(())
    }

    fn lower_match(
        &mut self,
        span: Span,
        subject: &hir::Expression,
        cases: &[hir::MatchCase],
    ) -> Result<(), String> {
        let subject = self.expression(subject)?;
        let finish = self.new_block();
        for case in cases {
            let tentative = self.value();
            self.emit(
                case.pattern.span,
                mir::OperationKind::Dictionary {
                    dest: tentative,
                    keys: vec![],
                    values: vec![],
                },
            );
            let matched = self.new_block();
            let next_case = self.new_block();
            self.lower_pattern_test(subject, &case.pattern, tentative, matched, next_case)?;

            self.current = matched;
            let mut bindings = Vec::new();
            collect_pattern_bindings(&case.pattern, &mut bindings);
            for (name, binding, binding_span) in bindings {
                let key = self.value();
                self.emit(
                    binding_span,
                    mir::OperationKind::Constant {
                        dest: key,
                        value: mir::Constant::String(name.to_owned()),
                    },
                );
                let value = self.value();
                self.emit(
                    binding_span,
                    mir::OperationKind::ItemGet {
                        dest: value,
                        collection: tentative,
                        index: key,
                    },
                );
                self.store_name(binding_span, name, binding, value)?;
            }

            if let Some(guard) = &case.guard {
                let body = self.new_block();
                let guard_value = self.expression(guard)?;
                self.terminate(mir::Terminator::Branch {
                    condition: guard_value,
                    then_target: body,
                    then_arguments: vec![],
                    else_target: next_case,
                    else_arguments: vec![],
                })?;
                self.current = body;
            }
            self.statements(&case.body)?;
            if self.is_open() {
                self.terminate(mir::Terminator::Jump {
                    target: finish,
                    arguments: vec![],
                })?;
            }
            self.current = next_case;
        }
        if self.is_open() {
            self.terminate(mir::Terminator::Jump {
                target: finish,
                arguments: vec![],
            })?;
        }
        self.current = finish;
        let _ = span;
        Ok(())
    }

    fn lower_pattern_test(
        &mut self,
        subject: mir::ValueId,
        pattern: &hir::Pattern,
        tentative: mir::ValueId,
        success: mir::BlockId,
        failure: mir::BlockId,
    ) -> Result<(), String> {
        match &pattern.kind {
            hir::PatternKind::Value(value) => {
                let value = self.expression(value)?;
                let matched = self.value();
                self.emit(
                    pattern.span,
                    mir::OperationKind::Compare {
                        dest: matched,
                        op: mir::CompareOperator::Equal,
                        left: subject,
                        right: value,
                    },
                );
                self.terminate(mir::Terminator::Branch {
                    condition: matched,
                    then_target: success,
                    then_arguments: vec![],
                    else_target: failure,
                    else_arguments: vec![],
                })?;
            }
            hir::PatternKind::SingletonNone | hir::PatternKind::SingletonBool(_) => {
                let expected = self.value();
                let value = match pattern.kind {
                    hir::PatternKind::SingletonNone => mir::Constant::None,
                    hir::PatternKind::SingletonBool(value) => mir::Constant::Bool(value),
                    _ => unreachable!(),
                };
                self.emit(
                    pattern.span,
                    mir::OperationKind::Constant {
                        dest: expected,
                        value,
                    },
                );
                let matched = self.value();
                self.emit(
                    pattern.span,
                    mir::OperationKind::Compare {
                        dest: matched,
                        op: mir::CompareOperator::Is,
                        left: subject,
                        right: expected,
                    },
                );
                self.terminate(mir::Terminator::Branch {
                    condition: matched,
                    then_target: success,
                    then_arguments: vec![],
                    else_target: failure,
                    else_arguments: vec![],
                })?;
            }
            hir::PatternKind::Capture { name, .. } => {
                self.record_pattern_capture(pattern.span, tentative, name, subject)?;
                self.terminate(mir::Terminator::Jump {
                    target: success,
                    arguments: vec![],
                })?;
            }
            hir::PatternKind::Wildcard => {
                self.terminate(mir::Terminator::Jump {
                    target: success,
                    arguments: vec![],
                })?;
            }
            hir::PatternKind::As {
                pattern: inner,
                name,
                ..
            } => {
                let inner_success = self.new_block();
                self.lower_pattern_test(subject, inner, tentative, inner_success, failure)?;
                self.current = inner_success;
                self.record_pattern_capture(pattern.span, tentative, name, subject)?;
                self.terminate(mir::Terminator::Jump {
                    target: success,
                    arguments: vec![],
                })?;
            }
            hir::PatternKind::Or(patterns) => {
                for (index, alternative) in patterns.iter().enumerate() {
                    let alternative_failure = if index + 1 == patterns.len() {
                        failure
                    } else {
                        self.new_block()
                    };
                    self.lower_pattern_test(
                        subject,
                        alternative,
                        tentative,
                        success,
                        alternative_failure,
                    )?;
                    if index + 1 != patterns.len() {
                        self.current = alternative_failure;
                    }
                }
            }
            hir::PatternKind::Sequence(patterns) => {
                let star = patterns
                    .iter()
                    .position(|pattern| matches!(pattern.kind, hir::PatternKind::Star(_)));
                let before_count = u32::try_from(star.unwrap_or(patterns.len()))
                    .map_err(|_| "sequence pattern is too large")?;
                let after_count =
                    u32::try_from(star.map_or(0, |index| patterns.len().saturating_sub(index + 1)))
                        .map_err(|_| "sequence pattern is too large")?;
                let values = self.value();
                let matched = self.value();
                self.emit(
                    pattern.span,
                    mir::OperationKind::PatternSequence {
                        values,
                        matched,
                        subject,
                        before_count,
                        after_count,
                        starred: star.is_some(),
                    },
                );
                let extracted = self.new_block();
                self.terminate(mir::Terminator::Branch {
                    condition: matched,
                    then_target: extracted,
                    then_arguments: vec![],
                    else_target: failure,
                    else_arguments: vec![],
                })?;
                self.current = extracted;
                self.lower_extracted_patterns(values, patterns, tentative, success, failure)?;
            }
            hir::PatternKind::Star(name) => {
                if let Some((name, _)) = name {
                    self.record_pattern_capture(pattern.span, tentative, name, subject)?;
                }
                self.terminate(mir::Terminator::Jump {
                    target: success,
                    arguments: vec![],
                })?;
            }
            hir::PatternKind::Mapping {
                keys,
                patterns,
                rest,
            } => {
                let preflight = self.value();
                self.emit(
                    pattern.span,
                    mir::OperationKind::PatternMappingCheck {
                        matched: preflight,
                        subject,
                        minimum_count: u32::try_from(keys.len())
                            .map_err(|_| "mapping pattern is too large")?,
                    },
                );
                let keys_ready = self.new_block();
                self.terminate(mir::Terminator::Branch {
                    condition: preflight,
                    then_target: keys_ready,
                    then_arguments: vec![],
                    else_target: failure,
                    else_arguments: vec![],
                })?;
                self.current = keys_ready;
                let keys = keys
                    .iter()
                    .map(|key| self.expression(key))
                    .collect::<Result<Vec<_>, _>>()?;
                let values = self.value();
                let matched = self.value();
                self.emit(
                    pattern.span,
                    mir::OperationKind::PatternMapping {
                        values,
                        matched,
                        subject,
                        keys,
                        rest: rest.is_some(),
                    },
                );
                let extracted = self.new_block();
                self.terminate(mir::Terminator::Branch {
                    condition: matched,
                    then_target: extracted,
                    then_arguments: vec![],
                    else_target: failure,
                    else_arguments: vec![],
                })?;
                self.current = extracted;
                let after_children = if rest.is_some() {
                    self.new_block()
                } else {
                    success
                };
                self.lower_extracted_patterns(
                    values,
                    patterns,
                    tentative,
                    after_children,
                    failure,
                )?;
                if let Some((name, _)) = rest {
                    self.current = after_children;
                    let rest_value = self.value();
                    self.emit(
                        pattern.span,
                        mir::OperationKind::ValueArrayGet {
                            dest: rest_value,
                            array: values,
                            index: u32::try_from(patterns.len())
                                .map_err(|_| "mapping pattern is too large")?,
                        },
                    );
                    self.record_pattern_capture(pattern.span, tentative, name, rest_value)?;
                    self.terminate(mir::Terminator::Jump {
                        target: success,
                        arguments: vec![],
                    })?;
                }
            }
            hir::PatternKind::Class {
                class,
                positional,
                keywords,
            } => {
                let class = self.expression(class)?;
                let values = self.value();
                let matched = self.value();
                self.emit(
                    pattern.span,
                    mir::OperationKind::PatternClass {
                        values,
                        matched,
                        subject,
                        class,
                        positional_count: u32::try_from(positional.len())
                            .map_err(|_| "class pattern is too large")?,
                        keyword_names: keywords.iter().map(|(name, _)| name.clone()).collect(),
                    },
                );
                let extracted = self.new_block();
                self.terminate(mir::Terminator::Branch {
                    condition: matched,
                    then_target: extracted,
                    then_arguments: vec![],
                    else_target: failure,
                    else_arguments: vec![],
                })?;
                self.current = extracted;
                let patterns = positional
                    .iter()
                    .chain(keywords.iter().map(|(_, pattern)| pattern))
                    .collect::<Vec<_>>();
                self.lower_extracted_pattern_refs(values, &patterns, tentative, success, failure)?;
            }
        }
        Ok(())
    }

    fn record_pattern_capture(
        &mut self,
        span: Span,
        tentative: mir::ValueId,
        name: &str,
        value: mir::ValueId,
    ) -> Result<(), String> {
        let key = self.value();
        self.emit(
            span,
            mir::OperationKind::Constant {
                dest: key,
                value: mir::Constant::String(name.to_owned()),
            },
        );
        self.emit(
            span,
            mir::OperationKind::DictionaryInsert {
                dictionary: tentative,
                key,
                value,
            },
        );
        Ok(())
    }

    fn lower_extracted_patterns(
        &mut self,
        values: mir::ValueId,
        patterns: &[hir::Pattern],
        tentative: mir::ValueId,
        success: mir::BlockId,
        failure: mir::BlockId,
    ) -> Result<(), String> {
        let patterns = patterns.iter().collect::<Vec<_>>();
        self.lower_extracted_pattern_refs(values, &patterns, tentative, success, failure)
    }

    fn lower_extracted_pattern_refs(
        &mut self,
        values: mir::ValueId,
        patterns: &[&hir::Pattern],
        tentative: mir::ValueId,
        success: mir::BlockId,
        failure: mir::BlockId,
    ) -> Result<(), String> {
        if patterns.is_empty() {
            self.terminate(mir::Terminator::Jump {
                target: success,
                arguments: vec![],
            })?;
            return Ok(());
        }
        for (index, pattern) in patterns.iter().enumerate() {
            let value = self.value();
            self.emit(
                pattern.span,
                mir::OperationKind::ValueArrayGet {
                    dest: value,
                    array: values,
                    index: u32::try_from(index).map_err(|_| "pattern is too large")?,
                },
            );
            let next = if index + 1 == patterns.len() {
                success
            } else {
                self.new_block()
            };
            self.lower_pattern_test(value, pattern, tentative, next, failure)?;
            if index + 1 != patterns.len() {
                self.current = next;
            }
        }
        Ok(())
    }

    fn lower_for(
        &mut self,
        span: Span,
        target: &hir::Target,
        iterable: &hir::Expression,
        body: &[hir::Statement],
        else_body: &[hir::Statement],
    ) -> Result<(), String> {
        let iterable = self.expression(iterable)?;
        let iterator = self.value();
        self.emit(
            span,
            mir::OperationKind::IteratorNew {
                dest: iterator,
                value: iterable,
            },
        );
        let header = self.new_block();
        let body_block = self.new_block();
        let exhausted = self.new_block();
        let exit = self.new_block();
        self.terminate(mir::Terminator::Jump {
            target: header,
            arguments: vec![],
        })?;
        self.current = header;
        let item = self.value();
        let has_value = self.value();
        self.emit(
            span,
            mir::OperationKind::IteratorNext {
                item,
                has_value,
                iterator,
            },
        );
        self.terminate(mir::Terminator::Branch {
            condition: has_value,
            then_target: body_block,
            then_arguments: vec![],
            else_target: exhausted,
            else_arguments: vec![],
        })?;
        self.current = body_block;
        self.write_clause_target(target, item)?;
        self.loops.push(LoopTargets {
            continue_target: header,
            break_target: exit,
            cleanup_depth: self.cleanups.len(),
        });
        self.statements(body)?;
        self.loops.pop();
        if self.is_open() {
            self.terminate(mir::Terminator::Jump {
                target: header,
                arguments: vec![],
            })?;
        }
        self.current = exhausted;
        self.statements(else_body)?;
        if self.is_open() {
            self.terminate(mir::Terminator::Jump {
                target: exit,
                arguments: vec![],
            })?;
        }
        self.current = exit;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_comprehension_level(
        &mut self,
        kind: hir::ComprehensionKind,
        element: &hir::Expression,
        key: Option<&hir::Expression>,
        clauses: &[hir::ComprehensionClause],
        index: usize,
        provided_iterator: Option<mir::ValueId>,
        result: mir::ValueId,
        exhausted_target: mir::BlockId,
    ) -> Result<(), String> {
        let clause = clauses
            .get(index)
            .ok_or_else(|| "comprehension clause index is out of range".to_owned())?;
        let iterator = if let Some(iterator) = provided_iterator {
            iterator
        } else {
            let iterable_expression = clause
                .iterable
                .as_ref()
                .ok_or_else(|| "nested comprehension clause is missing its iterable".to_owned())?;
            let iterable = self.expression(iterable_expression)?;
            let iterator = self.value();
            self.emit(
                iterable_expression.span,
                mir::OperationKind::IteratorNew {
                    dest: iterator,
                    value: iterable,
                },
            );
            iterator
        };
        let header = self.new_block();
        let body = self.new_block();
        let exhausted = self.new_block();
        self.terminate(mir::Terminator::Jump {
            target: header,
            arguments: Vec::new(),
        })?;
        self.current = header;
        let item = self.value();
        let has_value = self.value();
        self.emit(
            clause.target.span,
            mir::OperationKind::IteratorNext {
                item,
                has_value,
                iterator,
            },
        );
        self.terminate(mir::Terminator::Branch {
            condition: has_value,
            then_target: body,
            then_arguments: Vec::new(),
            else_target: exhausted,
            else_arguments: Vec::new(),
        })?;
        self.current = body;
        self.write_clause_target(&clause.target, item)?;
        for filter in &clause.filters {
            let condition = self.expression(filter)?;
            let passed = self.new_block();
            self.terminate(mir::Terminator::Branch {
                condition,
                then_target: passed,
                then_arguments: Vec::new(),
                else_target: header,
                else_arguments: Vec::new(),
            })?;
            self.current = passed;
        }
        if index + 1 < clauses.len() {
            self.lower_comprehension_level(
                kind,
                element,
                key,
                clauses,
                index + 1,
                None,
                result,
                header,
            )?;
        } else {
            match kind {
                hir::ComprehensionKind::List => {
                    let value = self.expression(element)?;
                    self.emit(
                        element.span,
                        mir::OperationKind::ListAppend {
                            list: result,
                            value,
                        },
                    );
                }
                hir::ComprehensionKind::Set => {
                    let value = self.expression(element)?;
                    self.emit(
                        element.span,
                        mir::OperationKind::SetInsert { set: result, value },
                    );
                }
                hir::ComprehensionKind::Dictionary => {
                    let key = key.ok_or_else(|| {
                        "dictionary comprehension is missing its key expression".to_owned()
                    })?;
                    let key_value = self.expression(key)?;
                    let mapped_value = self.expression(element)?;
                    self.emit(
                        element.span,
                        mir::OperationKind::DictionaryInsert {
                            dictionary: result,
                            key: key_value,
                            value: mapped_value,
                        },
                    );
                }
                hir::ComprehensionKind::Generator => {
                    let value = self.expression(element)?;
                    self.terminate(mir::Terminator::Yield {
                        value,
                        resume_value: None,
                        resume_target: header,
                        exception_target: self.exception_target,
                        delegate: None,
                    })?;
                }
            }
            if kind != hir::ComprehensionKind::Generator {
                self.terminate(mir::Terminator::Jump {
                    target: header,
                    arguments: Vec::new(),
                })?;
            }
        }
        self.current = exhausted;
        self.terminate(mir::Terminator::Jump {
            target: exhausted_target,
            arguments: Vec::new(),
        })?;
        Ok(())
    }

    fn lower_try(
        &mut self,
        body: &[hir::Statement],
        handlers: &[hir::ExceptionHandler],
        else_body: &[hir::Statement],
        finally_body: &[hir::Statement],
    ) -> Result<(), String> {
        let outer_exception = self.exception_target;
        let join = self.new_block();
        let normal_target = if finally_body.is_empty() {
            join
        } else {
            self.new_block()
        };
        let finally_exception = (!finally_body.is_empty()).then(|| self.new_block());
        let inner_exception = finally_exception.or(outer_exception);
        let dispatch = (!handlers.is_empty()).then(|| self.new_block());

        let finally_cleanup = (!finally_body.is_empty()).then(|| CleanupAction::Finally {
            statements: finally_body.to_vec(),
            exception_target: outer_exception,
        });
        if let Some(cleanup) = &finally_cleanup {
            self.cleanups.push(cleanup.clone());
        }

        self.exception_target = dispatch.or(inner_exception);
        self.statements(body)?;
        if self.is_open() {
            let else_block = self.new_block();
            self.terminate(mir::Terminator::Jump {
                target: else_block,
                arguments: vec![],
            })?;
            self.current = else_block;
            self.exception_target = inner_exception;
            self.statements(else_body)?;
            if self.is_open() {
                self.terminate(mir::Terminator::Jump {
                    target: normal_target,
                    arguments: vec![],
                })?;
            }
        }

        if let Some(dispatch) = dispatch {
            self.current = dispatch;
            self.exception_target = inner_exception;
            let active = self.value();
            self.emit(
                Span::default(),
                mir::OperationKind::ExceptionActive { dest: active },
            );
            let mut next_test = dispatch;
            for (index, handler) in handlers.iter().enumerate() {
                if index != 0 {
                    self.current = next_test;
                }
                let selected = self.new_block();
                if let Some(expected) = &handler.exception_type {
                    let expected = self.expression(expected)?;
                    let matches = self.value();
                    self.emit(
                        Span::default(),
                        mir::OperationKind::ExceptionMatches {
                            dest: matches,
                            exception: active,
                            expected_type: expected,
                        },
                    );
                    next_test = self.new_block();
                    self.terminate(mir::Terminator::Branch {
                        condition: matches,
                        then_target: selected,
                        then_arguments: vec![],
                        else_target: next_test,
                        else_arguments: vec![],
                    })?;
                } else {
                    self.terminate(mir::Terminator::Jump {
                        target: selected,
                        arguments: vec![],
                    })?;
                    next_test = self.new_block();
                }

                self.current = selected;
                let handled = self.value();
                self.emit(
                    Span::default(),
                    mir::OperationKind::HandlerEnter { dest: handled },
                );
                if let Some((name, binding)) = &handler.name {
                    self.store_name(Span::default(), name, *binding, handled)?;
                }
                let cleanup_error = self.new_block();
                self.exception_target = Some(cleanup_error);
                self.cleanups.push(CleanupAction::Handler {
                    binding: handler.name.clone(),
                    exception_target: inner_exception,
                });
                self.statements(&handler.body)?;
                self.cleanups.pop();
                if self.is_open() {
                    if let Some((name, binding)) = &handler.name {
                        self.clear_name(Span::default(), name, *binding)?;
                    }
                    self.exception_target = inner_exception;
                    self.emit(Span::default(), mir::OperationKind::HandlerLeave);
                    self.terminate(mir::Terminator::Jump {
                        target: normal_target,
                        arguments: vec![],
                    })?;
                }
                self.current = cleanup_error;
                self.exception_target = inner_exception;
                if let Some((name, binding)) = &handler.name {
                    self.clear_name(Span::default(), name, *binding)?;
                }
                self.emit(Span::default(), mir::OperationKind::HandlerLeave);
                self.emit(Span::default(), mir::OperationKind::Propagate);
                self.terminate(mir::Terminator::Jump {
                    target: join,
                    arguments: vec![],
                })?;
            }
            self.current = next_test;
            self.exception_target = inner_exception;
            self.emit(Span::default(), mir::OperationKind::Propagate);
            self.terminate(mir::Terminator::Jump {
                target: join,
                arguments: vec![],
            })?;
        }

        if finally_cleanup.is_some() {
            self.cleanups.pop();
        }

        if let Some(finally_exception) = finally_exception {
            self.current = finally_exception;
            self.exception_target = outer_exception;
            self.statements(finally_body)?;
            if self.is_open() {
                self.emit(Span::default(), mir::OperationKind::Propagate);
                self.terminate(mir::Terminator::Jump {
                    target: join,
                    arguments: vec![],
                })?;
            }
            self.current = normal_target;
            self.exception_target = outer_exception;
            self.statements(finally_body)?;
            if self.is_open() {
                self.terminate(mir::Terminator::Jump {
                    target: join,
                    arguments: vec![],
                })?;
            }
        }
        self.current = join;
        self.exception_target = outer_exception;
        Ok(())
    }

    fn lower_try_star(
        &mut self,
        body: &[hir::Statement],
        handlers: &[hir::ExceptionHandler],
        else_body: &[hir::Statement],
        finally_body: &[hir::Statement],
    ) -> Result<(), String> {
        let outer_exception = self.exception_target;
        let join = self.new_block();
        let normal_target = if finally_body.is_empty() {
            join
        } else {
            self.new_block()
        };
        let finally_exception = (!finally_body.is_empty()).then(|| self.new_block());
        let inner_exception = finally_exception.or(outer_exception);
        let dispatch = self.new_block();
        let finally_cleanup = (!finally_body.is_empty()).then(|| CleanupAction::Finally {
            statements: finally_body.to_vec(),
            exception_target: outer_exception,
        });
        if let Some(cleanup) = &finally_cleanup {
            self.cleanups.push(cleanup.clone());
        }

        self.exception_target = Some(dispatch);
        self.statements(body)?;
        if self.is_open() {
            let else_block = self.new_block();
            self.terminate(mir::Terminator::Jump {
                target: else_block,
                arguments: vec![],
            })?;
            self.current = else_block;
            self.exception_target = inner_exception;
            self.statements(else_body)?;
            if self.is_open() {
                self.terminate(mir::Terminator::Jump {
                    target: normal_target,
                    arguments: vec![],
                })?;
            }
        }

        self.current = dispatch;
        self.exception_target = inner_exception;
        let mut remaining = self.value();
        self.emit(
            Span::default(),
            mir::OperationKind::ExceptionActive { dest: remaining },
        );
        let no_error = self.value();
        self.emit(
            Span::default(),
            mir::OperationKind::Constant {
                dest: no_error,
                value: mir::Constant::None,
            },
        );
        let errors = self.value();
        self.emit(
            Span::default(),
            mir::OperationKind::CellNew {
                dest: errors,
                initial: Some(no_error),
            },
        );
        for handler in handlers {
            let expected = handler
                .exception_type
                .as_ref()
                .ok_or_else(|| "except-star handler requires an exception type".to_owned())?;
            let expected = self.expression(expected)?;
            let split = self.value();
            self.emit(
                Span::default(),
                mir::OperationKind::ExceptionSplit {
                    dest: split,
                    exception: remaining,
                    expected_type: expected,
                },
            );
            let matched = self.value();
            self.emit(
                Span::default(),
                mir::OperationKind::ValueArrayGet {
                    dest: matched,
                    array: split,
                    index: 0,
                },
            );
            let rest = self.value();
            self.emit(
                Span::default(),
                mir::OperationKind::ValueArrayGet {
                    dest: rest,
                    array: split,
                    index: 1,
                },
            );
            let selected = self.new_block();
            let next = self.new_block();
            self.terminate(mir::Terminator::Branch {
                condition: matched,
                then_target: selected,
                then_arguments: vec![],
                else_target: next,
                else_arguments: vec![],
            })?;

            self.current = selected;
            self.emit(
                Span::default(),
                mir::OperationKind::ExceptionSetActive { exception: matched },
            );
            let handled = self.value();
            self.emit(
                Span::default(),
                mir::OperationKind::HandlerEnter { dest: handled },
            );
            if let Some((name, binding)) = &handler.name {
                self.store_name(Span::default(), name, *binding, handled)?;
            }
            let cleanup_error = self.new_block();
            self.exception_target = Some(cleanup_error);
            self.cleanups.push(CleanupAction::Handler {
                binding: handler.name.clone(),
                exception_target: inner_exception,
            });
            self.statements(&handler.body)?;
            self.cleanups.pop();
            if self.is_open() {
                if let Some((name, binding)) = &handler.name {
                    self.clear_name(Span::default(), name, *binding)?;
                }
                self.exception_target = inner_exception;
                self.emit(Span::default(), mir::OperationKind::HandlerLeave);
                self.terminate(mir::Terminator::Jump {
                    target: next,
                    arguments: vec![],
                })?;
            }
            self.current = cleanup_error;
            self.exception_target = inner_exception;
            if let Some((name, binding)) = &handler.name {
                self.clear_name(Span::default(), name, *binding)?;
            }
            let raised = self.value();
            self.emit(
                Span::default(),
                mir::OperationKind::ExceptionActive { dest: raised },
            );
            self.emit(Span::default(), mir::OperationKind::HandlerLeave);
            let previous = self.value();
            self.emit(
                Span::default(),
                mir::OperationKind::CellGet {
                    dest: previous,
                    cell: errors,
                    name: None,
                    free: false,
                },
            );
            let combined = self.value();
            self.emit(
                Span::default(),
                mir::OperationKind::ExceptionCombine {
                    dest: combined,
                    left: previous,
                    right: raised,
                },
            );
            self.emit(
                Span::default(),
                mir::OperationKind::CellSet {
                    cell: errors,
                    value: combined,
                },
            );
            self.emit(Span::default(), mir::OperationKind::ExceptionClearActive);
            self.terminate(mir::Terminator::Jump {
                target: next,
                arguments: vec![],
            })?;

            self.current = next;
            self.exception_target = inner_exception;
            remaining = rest;
        }

        let accumulated = self.value();
        self.emit(
            Span::default(),
            mir::OperationKind::CellGet {
                dest: accumulated,
                cell: errors,
                name: None,
                free: false,
            },
        );
        let handler_failures = self.new_block();
        let no_handler_failures = self.new_block();
        self.terminate(mir::Terminator::Branch {
            condition: accumulated,
            then_target: handler_failures,
            then_arguments: vec![],
            else_target: no_handler_failures,
            else_arguments: vec![],
        })?;
        self.current = handler_failures;
        let merged = self.value();
        self.emit(
            Span::default(),
            mir::OperationKind::ExceptionCombine {
                dest: merged,
                left: accumulated,
                right: remaining,
            },
        );
        self.emit(
            Span::default(),
            mir::OperationKind::ExceptionSetActive { exception: merged },
        );
        self.emit(Span::default(), mir::OperationKind::Propagate);
        self.terminate(mir::Terminator::Jump {
            target: join,
            arguments: vec![],
        })?;
        self.current = no_handler_failures;
        let unmatched = self.new_block();
        let all_handled = self.new_block();
        self.terminate(mir::Terminator::Branch {
            condition: remaining,
            then_target: unmatched,
            then_arguments: vec![],
            else_target: all_handled,
            else_arguments: vec![],
        })?;
        self.current = unmatched;
        self.emit(
            Span::default(),
            mir::OperationKind::ExceptionSetActive {
                exception: remaining,
            },
        );
        self.emit(Span::default(), mir::OperationKind::Propagate);
        self.terminate(mir::Terminator::Jump {
            target: join,
            arguments: vec![],
        })?;
        self.current = all_handled;
        self.terminate(mir::Terminator::Jump {
            target: normal_target,
            arguments: vec![],
        })?;

        if finally_cleanup.is_some() {
            self.cleanups.pop();
        }
        if let Some(finally_exception) = finally_exception {
            self.current = finally_exception;
            self.exception_target = outer_exception;
            self.statements(finally_body)?;
            if self.is_open() {
                self.emit(Span::default(), mir::OperationKind::Propagate);
                self.terminate(mir::Terminator::Jump {
                    target: join,
                    arguments: vec![],
                })?;
            }
            self.current = normal_target;
            self.exception_target = outer_exception;
            self.statements(finally_body)?;
            if self.is_open() {
                self.terminate(mir::Terminator::Jump {
                    target: join,
                    arguments: vec![],
                })?;
            }
        }
        self.current = join;
        self.exception_target = outer_exception;
        Ok(())
    }

    fn emit_cleanups_from(&mut self, floor: usize) -> Result<(), String> {
        let original = self.cleanups.clone();
        for index in (floor..original.len()).rev() {
            self.cleanups = original[..index].to_vec();
            match &original[index] {
                CleanupAction::Handler {
                    binding,
                    exception_target,
                } => {
                    self.exception_target = *exception_target;
                    if let Some((name, binding)) = binding {
                        self.clear_name(Span::default(), name, *binding)?;
                    }
                    self.emit(Span::default(), mir::OperationKind::HandlerLeave);
                }
                CleanupAction::Finally {
                    statements,
                    exception_target,
                } => {
                    self.exception_target = *exception_target;
                    self.statements(statements)?;
                }
                CleanupAction::ClassFinally {
                    namespace,
                    members,
                    exception_target,
                } => {
                    self.exception_target = *exception_target;
                    self.lower_class_assignment_members(Span::default(), *namespace, members)?;
                }
                CleanupAction::ClassHandler {
                    binding,
                    exception_target,
                } => {
                    self.exception_target = *exception_target;
                    if let Some((name, binding)) = binding {
                        self.clear_name(Span::default(), name, *binding)?;
                    }
                    self.emit(Span::default(), mir::OperationKind::HandlerLeave);
                }
            }
            if !self.is_open() {
                return Ok(());
            }
        }
        self.cleanups = original;
        Ok(())
    }

    fn clear_name(&mut self, span: Span, name: &str, binding: hir::Binding) -> Result<(), String> {
        match binding {
            hir::Binding::Global => self.emit(
                span,
                mir::OperationKind::GlobalDelete {
                    name: name.to_owned(),
                },
            ),
            hir::Binding::Local | hir::Binding::Cell | hir::Binding::Free => {
                let cell = self
                    .cells
                    .get(name)
                    .copied()
                    .ok_or_else(|| format!("local `{name}` has no cell"))?;
                self.emit(span, mir::OperationKind::CellClear { cell });
            }
            hir::Binding::ClassName | hir::Binding::ClassFree => {
                let namespace = self
                    .class_scopes
                    .last()
                    .map(|scope| scope.namespace)
                    .ok_or_else(|| format!("class binding `{name}` escaped class lowering"))?;
                self.emit(
                    span,
                    mir::OperationKind::ClassNamespaceDelete {
                        namespace,
                        name: name.to_owned(),
                    },
                );
            }
        }
        Ok(())
    }

    fn lower_yield_expression(
        &mut self,
        span: Span,
        value: Option<&hir::Expression>,
    ) -> Result<mir::ValueId, String> {
        let yielded = if let Some(value) = value {
            self.expression(value)?
        } else {
            let value = self.value();
            self.emit(
                span,
                mir::OperationKind::Constant {
                    dest: value,
                    value: mir::Constant::None,
                },
            );
            value
        };
        let resume_target = self.new_block();
        let resume_value = self.value();
        self.blocks[resume_target.0 as usize]
            .parameters
            .push(resume_value);
        self.terminate(mir::Terminator::Yield {
            value: yielded,
            resume_value: Some(resume_value),
            resume_target,
            exception_target: self.exception_target,
            delegate: None,
        })?;
        self.current = resume_target;
        Ok(resume_value)
    }

    fn lower_yield_from_expression(
        &mut self,
        span: Span,
        value: &hir::Expression,
    ) -> Result<mir::ValueId, String> {
        let iterable = self.expression(value)?;
        let iterator = self.value();
        self.emit(
            span,
            mir::OperationKind::IteratorNew {
                dest: iterator,
                value: iterable,
            },
        );
        let yielded = self.value();
        let result = self.value();
        let complete = self.value();
        self.emit(
            span,
            mir::OperationKind::YieldFromNext {
                yielded,
                result,
                complete,
                iterator,
            },
        );
        let join = self.new_block();
        let join_result = self.value();
        self.blocks[join.0 as usize].parameters.push(join_result);
        let suspend = self.new_block();
        self.terminate(mir::Terminator::Branch {
            condition: complete,
            then_target: join,
            then_arguments: vec![result],
            else_target: suspend,
            else_arguments: Vec::new(),
        })?;
        self.current = suspend;
        self.terminate(mir::Terminator::Yield {
            value: yielded,
            resume_value: Some(join_result),
            resume_target: join,
            exception_target: self.exception_target,
            delegate: Some(iterator),
        })?;
        self.current = join;
        Ok(join_result)
    }

    fn expression(&mut self, expression: &hir::Expression) -> Result<mir::ValueId, String> {
        let kind = match &expression.kind {
            hir::ExpressionKind::None => mir::OperationKind::Constant {
                dest: self.value(),
                value: mir::Constant::None,
            },
            hir::ExpressionKind::Bool(value) => mir::OperationKind::Constant {
                dest: self.value(),
                value: mir::Constant::Bool(*value),
            },
            hir::ExpressionKind::Int(value) => mir::OperationKind::Constant {
                dest: self.value(),
                value: mir::Constant::Int(value.clone()),
            },
            hir::ExpressionKind::Float(value) => mir::OperationKind::Constant {
                dest: self.value(),
                value: mir::Constant::Float(value.to_bits()),
            },
            hir::ExpressionKind::String(value) => mir::OperationKind::Constant {
                dest: self.value(),
                value: mir::Constant::String(value.clone()),
            },
            hir::ExpressionKind::Bytes(value) => mir::OperationKind::Constant {
                dest: self.value(),
                value: mir::Constant::Bytes(value.clone()),
            },
            hir::ExpressionKind::Complex { real, imag } => mir::OperationKind::Constant {
                dest: self.value(),
                value: mir::Constant::Complex {
                    real: real.to_bits(),
                    imag: imag.to_bits(),
                },
            },
            hir::ExpressionKind::Slice { start, stop, step } => mir::OperationKind::SliceNew {
                dest: self.value(),
                start: start
                    .as_deref()
                    .map(|value| self.expression(value))
                    .transpose()?,
                stop: stop
                    .as_deref()
                    .map(|value| self.expression(value))
                    .transpose()?,
                step: step
                    .as_deref()
                    .map(|value| self.expression(value))
                    .transpose()?,
            },
            hir::ExpressionKind::List(values) => {
                let values = values
                    .iter()
                    .map(|value| self.expression(value))
                    .collect::<Result<Vec<_>, _>>()?;
                mir::OperationKind::List {
                    dest: self.value(),
                    values,
                }
            }
            hir::ExpressionKind::Tuple(values) => {
                let values = values
                    .iter()
                    .map(|value| self.expression(value))
                    .collect::<Result<Vec<_>, _>>()?;
                mir::OperationKind::Tuple {
                    dest: self.value(),
                    values,
                }
            }
            hir::ExpressionKind::Dictionary(entries) => {
                let dictionary = self.value();
                self.emit(
                    expression.span,
                    mir::OperationKind::Dictionary {
                        dest: dictionary,
                        keys: Vec::new(),
                        values: Vec::new(),
                    },
                );
                for entry in entries {
                    match entry {
                        hir::DictionaryEntry::Pair { key, value } => {
                            let key_value = self.expression(key)?;
                            let mapped_value = self.expression(value)?;
                            self.emit(
                                value.span,
                                mir::OperationKind::ItemSet {
                                    collection: dictionary,
                                    index: key_value,
                                    value: mapped_value,
                                },
                            );
                        }
                        hir::DictionaryEntry::Unpack(source) => {
                            let source_value = self.expression(source)?;
                            self.emit(
                                source.span,
                                mir::OperationKind::DictionaryMerge {
                                    dictionary,
                                    source: source_value,
                                },
                            );
                        }
                    }
                }
                mir::OperationKind::Copy {
                    dest: self.value(),
                    source: dictionary,
                }
            }
            hir::ExpressionKind::Set(values) => {
                let values = values
                    .iter()
                    .map(|value| self.expression(value))
                    .collect::<Result<Vec<_>, _>>()?;
                mir::OperationKind::Set {
                    dest: self.value(),
                    values,
                }
            }
            hir::ExpressionKind::Subscript { value, index } => mir::OperationKind::ItemGet {
                dest: self.value(),
                collection: self.expression(value)?,
                index: self.expression(index)?,
            },
            hir::ExpressionKind::Attribute { value, name } => mir::OperationKind::AttributeGet {
                dest: self.value(),
                receiver: self.expression(value)?,
                name: name.clone(),
            },
            hir::ExpressionKind::Length { value } => mir::OperationKind::Length {
                dest: self.value(),
                value: self.expression(value)?,
            },
            hir::ExpressionKind::Range { start, stop, step } => mir::OperationKind::Range {
                dest: self.value(),
                start: self.expression(start)?,
                stop: self.expression(stop)?,
                step: self.expression(step)?,
            },
            hir::ExpressionKind::Lambda {
                parameters,
                body,
                locals,
                cells,
                free,
            } => {
                let return_statement = hir::Statement {
                    span: expression.span,
                    kind: hir::StatementKind::Return {
                        value: Some((**body).clone()),
                    },
                };
                let function = self.program.lower_scope(
                    "<lambda>".to_owned(),
                    self.child_qualified_name("<lambda>"),
                    parameters,
                    locals,
                    free,
                    &[return_statement],
                    false,
                )?;
                let mut defaults = Vec::new();
                for (index, parameter) in parameters.iter().enumerate() {
                    if let Some(default) = &parameter.default {
                        defaults.push((
                            u32::try_from(index).map_err(|_| "too many lambda parameters")?,
                            self.expression(default)?,
                        ));
                    }
                }
                let closure =
                    free.iter()
                        .map(|name| {
                            self.cells.get(name).copied().ok_or_else(|| {
                                format!("free variable `{name}` has no closure cell")
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                mir::OperationKind::MakeFunction {
                    dest: self.value(),
                    function,
                    defaults,
                    closure,
                    local_names: locals.clone(),
                    cell_names: cells.clone(),
                    free_names: free.clone(),
                }
            }
            hir::ExpressionKind::Name { name, binding } => match binding {
                hir::Binding::Global => mir::OperationKind::GlobalGet {
                    dest: self.value(),
                    name: name.clone(),
                },
                hir::Binding::ClassName => {
                    let namespace = self
                        .class_scopes
                        .last()
                        .map(|scope| scope.namespace)
                        .ok_or_else(|| format!("class name `{name}` escaped class lowering"))?;
                    mir::OperationKind::ClassNameGet {
                        dest: self.value(),
                        namespace,
                        name: name.clone(),
                    }
                }
                hir::Binding::ClassFree => {
                    let namespace = self
                        .class_scopes
                        .last()
                        .map(|scope| scope.namespace)
                        .ok_or_else(|| {
                            format!("class free name `{name}` escaped class lowering")
                        })?;
                    let cell =
                        self.cells.get(name).copied().ok_or_else(|| {
                            format!("class free name `{name}` has no closure cell")
                        })?;
                    mir::OperationKind::ClassFreeGet {
                        dest: self.value(),
                        namespace,
                        cell,
                        name: name.clone(),
                    }
                }
                hir::Binding::Local | hir::Binding::Cell | hir::Binding::Free => {
                    mir::OperationKind::CellGet {
                        dest: self.value(),
                        cell: self
                            .cells
                            .get(name)
                            .copied()
                            .ok_or_else(|| format!("name `{name}` has no cell"))?,
                        name: Some(name.clone()),
                        free: *binding == hir::Binding::Free,
                    }
                }
            },
            hir::ExpressionKind::Unary { op, operand } => {
                let operand = self.expression(operand)?;
                mir::OperationKind::Unary {
                    dest: self.value(),
                    op: match op {
                        hir::UnaryOperator::Positive => mir::UnaryOperator::Positive,
                        hir::UnaryOperator::Negate => mir::UnaryOperator::Negate,
                        hir::UnaryOperator::Invert => mir::UnaryOperator::Invert,
                        hir::UnaryOperator::Not => mir::UnaryOperator::Not,
                    },
                    operand,
                }
            }
            hir::ExpressionKind::Boolean { op, values } => {
                self.lower_boolean_expression(expression.span, *op, values)?
            }
            hir::ExpressionKind::Binary { op, left, right } => {
                let left = self.expression(left)?;
                let right = self.expression(right)?;
                mir::OperationKind::Binary {
                    dest: self.value(),
                    op: *op,
                    left,
                    right,
                }
            }
            hir::ExpressionKind::Compare { left, comparisons } => {
                self.lower_compare_expression(expression.span, left, comparisons)?
            }
            hir::ExpressionKind::Comprehension {
                kind,
                outer_iterable,
                element,
                key,
                clauses,
                locals,
                cells,
                free,
            } => {
                let outer_iterable = self.expression(outer_iterable)?;
                let outer_iterator = self.value();
                self.emit(
                    expression.span,
                    mir::OperationKind::IteratorNew {
                        dest: outer_iterator,
                        value: outer_iterable,
                    },
                );
                let function = self.program.lower_comprehension(
                    *kind,
                    element,
                    key.as_deref(),
                    clauses,
                    locals,
                    free,
                    self.qualname_prefix.clone(),
                    self.nested_uses_locals,
                )?;
                let closure =
                    free.iter()
                        .map(|name| {
                            self.cells.get(name).copied().ok_or_else(|| {
                                format!("free variable `{name}` has no closure cell")
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                let callable = self.value();
                self.emit(
                    expression.span,
                    mir::OperationKind::MakeFunction {
                        dest: callable,
                        function,
                        defaults: Vec::new(),
                        closure,
                        local_names: locals.clone(),
                        cell_names: cells.clone(),
                        free_names: free.clone(),
                    },
                );
                mir::OperationKind::Call {
                    dest: self.value(),
                    callable,
                    positional: vec![outer_iterator],
                    keywords: Vec::new(),
                }
            }
            hir::ExpressionKind::Yield { value } => {
                return self.lower_yield_expression(expression.span, value.as_deref());
            }
            hir::ExpressionKind::YieldFrom { value } => {
                return self.lower_yield_from_expression(expression.span, value);
            }
            hir::ExpressionKind::NamedExpression {
                name,
                binding,
                value,
            } => {
                let stored = self.expression(value)?;
                if *binding == hir::Binding::Global
                    && let Some(namespace) = self.class_scopes.last().map(|scope| scope.namespace)
                {
                    self.emit(
                        expression.span,
                        mir::OperationKind::ClassNamespaceSet {
                            namespace,
                            name: name.clone(),
                            value: stored,
                        },
                    );
                    self.record_class_name(name);
                } else {
                    self.store_name(expression.span, name, *binding, stored)?;
                }
                mir::OperationKind::Copy {
                    dest: self.value(),
                    source: stored,
                }
            }
            hir::ExpressionKind::JoinedString(values) => {
                if values.is_empty() {
                    mir::OperationKind::Constant {
                        dest: self.value(),
                        value: mir::Constant::String(String::new()),
                    }
                } else {
                    let mut accumulated = self.expression(&values[0])?;
                    for value in &values[1..] {
                        let right = self.expression(value)?;
                        let next = self.value();
                        self.emit(
                            value.span,
                            mir::OperationKind::Binary {
                                dest: next,
                                op: mir::BinaryOperator::Add,
                                left: accumulated,
                                right,
                            },
                        );
                        accumulated = next;
                    }
                    mir::OperationKind::Copy {
                        dest: self.value(),
                        source: accumulated,
                    }
                }
            }
            hir::ExpressionKind::FormattedValue {
                value,
                conversion,
                format_spec,
            } => {
                let value = self.expression(value)?;
                let spec = if let Some(format_spec) = format_spec {
                    self.expression(format_spec)?
                } else {
                    let spec = self.value();
                    self.emit(
                        expression.span,
                        mir::OperationKind::Constant {
                            dest: spec,
                            value: mir::Constant::String(String::new()),
                        },
                    );
                    spec
                };
                mir::OperationKind::FormatValue {
                    dest: self.value(),
                    value,
                    conversion: match conversion {
                        hir::FormatConversion::None => mir::FormatConversion::None,
                        hir::FormatConversion::Str => mir::FormatConversion::Str,
                        hir::FormatConversion::Repr => mir::FormatConversion::Repr,
                        hir::FormatConversion::Ascii => mir::FormatConversion::Ascii,
                    },
                    spec,
                }
            }
            hir::ExpressionKind::Call { callable, parts } => {
                let callable = self.expression(callable)?;
                let expanded = parts.iter().any(|part| {
                    matches!(
                        part,
                        hir::CallPart::Starred(_) | hir::CallPart::KeywordUnpack(_)
                    )
                });
                if expanded {
                    let arguments = self.value();
                    self.emit(
                        expression.span,
                        mir::OperationKind::CallArgumentsNew {
                            dest: arguments,
                            callable,
                        },
                    );
                    for part in parts {
                        let (kind, name, value) = match part {
                            hir::CallPart::Positional(value) => {
                                (mir::CallArgumentKind::Positional, None, value)
                            }
                            hir::CallPart::Starred(value) => {
                                (mir::CallArgumentKind::Starred, None, value)
                            }
                            hir::CallPart::Keyword { name, value } => {
                                (mir::CallArgumentKind::Keyword, Some(name.clone()), value)
                            }
                            hir::CallPart::KeywordUnpack(value) => {
                                (mir::CallArgumentKind::KeywordUnpack, None, value)
                            }
                        };
                        let part_value = self.expression(value)?;
                        self.emit(
                            value.span,
                            mir::OperationKind::CallArgumentAdd {
                                arguments,
                                kind,
                                name,
                                value: part_value,
                            },
                        );
                    }
                    mir::OperationKind::CallPrepared {
                        dest: self.value(),
                        callable,
                        arguments,
                    }
                } else {
                    let mut positional = Vec::new();
                    let mut keywords = Vec::new();
                    for part in parts {
                        match part {
                            hir::CallPart::Positional(value) => {
                                positional.push(self.expression(value)?);
                            }
                            hir::CallPart::Keyword { name, value } => {
                                keywords.push((name.clone(), self.expression(value)?));
                            }
                            hir::CallPart::Starred(_) | hir::CallPart::KeywordUnpack(_) => {
                                unreachable!("expanded call part missed prepared-call lowering")
                            }
                        }
                    }
                    mir::OperationKind::Call {
                        dest: self.value(),
                        callable,
                        positional,
                        keywords,
                    }
                }
            }
        };
        let destination = kind
            .destination()
            .ok_or_else(|| "expression did not define a value".to_owned())?;
        self.emit(expression.span, kind);
        Ok(destination)
    }

    fn emit_comparison(
        &mut self,
        span: Span,
        op: hir::CompareOperator,
        left: mir::ValueId,
        right: mir::ValueId,
    ) -> mir::ValueId {
        let dest = self.value();
        let kind = if matches!(op, hir::CompareOperator::In | hir::CompareOperator::NotIn) {
            mir::OperationKind::Contains {
                dest,
                collection: right,
                needle: left,
                negate: op == hir::CompareOperator::NotIn,
            }
        } else {
            mir::OperationKind::Compare {
                dest,
                op: match op {
                    hir::CompareOperator::Equal => mir::CompareOperator::Equal,
                    hir::CompareOperator::NotEqual => mir::CompareOperator::NotEqual,
                    hir::CompareOperator::Less => mir::CompareOperator::Less,
                    hir::CompareOperator::LessEqual => mir::CompareOperator::LessEqual,
                    hir::CompareOperator::Greater => mir::CompareOperator::Greater,
                    hir::CompareOperator::GreaterEqual => mir::CompareOperator::GreaterEqual,
                    hir::CompareOperator::In | hir::CompareOperator::NotIn => unreachable!(),
                    hir::CompareOperator::Is => mir::CompareOperator::Is,
                    hir::CompareOperator::IsNot => mir::CompareOperator::IsNot,
                },
                left,
                right,
            }
        };
        self.emit(span, kind);
        dest
    }

    fn lower_compare_expression(
        &mut self,
        _span: Span,
        left: &hir::Expression,
        comparisons: &[(hir::CompareOperator, hir::Expression)],
    ) -> Result<mir::OperationKind, String> {
        if comparisons.is_empty() {
            return Err("comparison expression has no operators".to_owned());
        }
        let mut left_value = self.expression(left)?;
        let join = self.new_block();
        let result = self.value();
        self.blocks[join.0 as usize].parameters.push(result);
        for (index, (op, right)) in comparisons.iter().enumerate() {
            let right_value = self.expression(right)?;
            let compared = self.emit_comparison(right.span, *op, left_value, right_value);
            if index + 1 == comparisons.len() {
                self.terminate(mir::Terminator::Jump {
                    target: join,
                    arguments: vec![compared],
                })?;
            } else {
                let next = self.new_block();
                let next_left = self.value();
                self.blocks[next.0 as usize].parameters.push(next_left);
                self.terminate(mir::Terminator::Branch {
                    condition: compared,
                    then_target: next,
                    then_arguments: vec![right_value],
                    else_target: join,
                    else_arguments: vec![compared],
                })?;
                self.current = next;
                left_value = next_left;
            }
        }
        self.current = join;
        Ok(mir::OperationKind::Copy {
            dest: self.value(),
            source: result,
        })
    }

    fn lower_boolean_expression(
        &mut self,
        _span: Span,
        op: hir::BooleanOperator,
        values: &[hir::Expression],
    ) -> Result<mir::OperationKind, String> {
        let (first, rest) = values
            .split_first()
            .ok_or_else(|| "boolean expression has no operands".to_owned())?;
        let mut current = self.expression(first)?;
        for value in rest {
            let evaluate_next = self.new_block();
            let join = self.new_block();
            let result = self.value();
            self.blocks[join.0 as usize].parameters.push(result);
            let (then_target, then_arguments, else_target, else_arguments) = match op {
                hir::BooleanOperator::And => (evaluate_next, vec![], join, vec![current]),
                hir::BooleanOperator::Or => (join, vec![current], evaluate_next, vec![]),
            };
            self.terminate(mir::Terminator::Branch {
                condition: current,
                then_target,
                then_arguments,
                else_target,
                else_arguments,
            })?;
            self.current = evaluate_next;
            let next = self.expression(value)?;
            self.terminate(mir::Terminator::Jump {
                target: join,
                arguments: vec![next],
            })?;
            self.current = join;
            current = result;
        }
        Ok(mir::OperationKind::Copy {
            dest: self.value(),
            source: current,
        })
    }

    fn class_decorator_expression(
        &mut self,
        expression: &hir::Expression,
        namespace: mir::ValueId,
    ) -> Result<mir::ValueId, String> {
        let hir::ExpressionKind::Attribute { value, name } = &expression.kind else {
            return self.expression(expression);
        };
        let hir::ExpressionKind::Name {
            name: local_name, ..
        } = &value.kind
        else {
            return self.expression(expression);
        };
        let local = self.value();
        self.emit(
            expression.span,
            mir::OperationKind::ClassNamespaceGet {
                dest: local,
                namespace,
                name: local_name.clone(),
            },
        );
        let destination = self.value();
        self.emit(
            expression.span,
            mir::OperationKind::AttributeGet {
                dest: destination,
                receiver: local,
                name: name.clone(),
            },
        );
        Ok(destination)
    }

    fn value(&mut self) -> mir::ValueId {
        let value = mir::ValueId(self.next_value);
        self.next_value += 1;
        value
    }

    fn new_block(&mut self) -> mir::BlockId {
        let id = mir::BlockId(
            u32::try_from(self.blocks.len()).expect("MIR block count must fit the BlockId ABI"),
        );
        self.blocks.push(mir::Block {
            parameters: vec![],
            operations: vec![],
            terminator: mir::Terminator::Unreachable,
        });
        id
    }

    fn emit(&mut self, span: Span, kind: mir::OperationKind) {
        let block = &mut self.blocks[self.current.0 as usize];
        let operation = block.operations.len();
        block.operations.push(mir::Operation { span, kind });
        if let Some(target) = self.exception_target {
            self.exception_edges.insert(
                (
                    self.current.0,
                    u32::try_from(operation).expect("MIR operation count must fit u32"),
                ),
                target,
            );
        }
    }

    fn terminate(&mut self, terminator: mir::Terminator) -> Result<(), String> {
        let block = &mut self.blocks[self.current.0 as usize];
        if !matches!(block.terminator, mir::Terminator::Unreachable) {
            return Err(format!(
                "MIR block {} already has a terminator",
                self.current.0
            ));
        }
        block.terminator = terminator;
        Ok(())
    }

    fn is_open(&self) -> bool {
        matches!(
            self.blocks[self.current.0 as usize].terminator,
            mir::Terminator::Unreachable
        )
    }
}

fn collect_pattern_bindings<'a>(
    pattern: &'a hir::Pattern,
    bindings: &mut Vec<(&'a str, hir::Binding, Span)>,
) {
    match &pattern.kind {
        hir::PatternKind::Capture { name, binding } => {
            bindings.push((name, *binding, pattern.span));
        }
        hir::PatternKind::As {
            pattern: inner,
            name,
            binding,
        } => {
            collect_pattern_bindings(inner, bindings);
            bindings.push((name, *binding, pattern.span));
        }
        hir::PatternKind::Or(patterns) => {
            if let Some(first) = patterns.first() {
                collect_pattern_bindings(first, bindings);
            }
        }
        hir::PatternKind::Sequence(patterns) => {
            for pattern in patterns {
                collect_pattern_bindings(pattern, bindings);
            }
        }
        hir::PatternKind::Star(Some((name, binding))) => {
            bindings.push((name, *binding, pattern.span));
        }
        hir::PatternKind::Star(None) => {}
        hir::PatternKind::Mapping { patterns, rest, .. } => {
            for pattern in patterns {
                collect_pattern_bindings(pattern, bindings);
            }
            if let Some((name, binding)) = rest {
                bindings.push((name, *binding, pattern.span));
            }
        }
        hir::PatternKind::Class {
            positional,
            keywords,
            ..
        } => {
            for pattern in positional {
                collect_pattern_bindings(pattern, bindings);
            }
            for (_, pattern) in keywords {
                collect_pattern_bindings(pattern, bindings);
            }
        }
        hir::PatternKind::Value(_)
        | hir::PatternKind::SingletonNone
        | hir::PatternKind::SingletonBool(_)
        | hir::PatternKind::Wildcard => {}
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::{sema, syntax};

    #[test]
    fn gate4_target_write_planner_evaluates_rhs_receiver_and_index_once_with_exception_edge() {
        let path = Path::new("gate4_target_write.py");
        let source = r#"
def receiver():
    return [0]

def index():
    return 0

def rhs():
    return 7

try:
    receiver()[index()] = rhs()
except Exception:
    marker = 1
"#;
        let syntax = syntax::parse(path, source).unwrap();
        let hir = sema::analyze(path, &syntax).unwrap();
        let program = lower(&hir).unwrap();
        let function = &program.functions[program.entry.0 as usize];

        let mut rhs_get = None;
        let mut receiver_get = None;
        let mut index_get = None;
        let mut item_set = None;
        let mut call_count = 0;
        for (block_index, block) in function.blocks.iter().enumerate() {
            for (operation_index, operation) in block.operations.iter().enumerate() {
                match &operation.kind {
                    mir::OperationKind::GlobalGet { name, .. } if name == "rhs" => {
                        assert!(rhs_get.replace((block_index, operation_index)).is_none());
                    }
                    mir::OperationKind::GlobalGet { name, .. } if name == "receiver" => {
                        assert!(
                            receiver_get
                                .replace((block_index, operation_index))
                                .is_none()
                        );
                    }
                    mir::OperationKind::GlobalGet { name, .. } if name == "index" => {
                        assert!(index_get.replace((block_index, operation_index)).is_none());
                    }
                    mir::OperationKind::Call { .. } => call_count += 1,
                    mir::OperationKind::ItemSet { .. } => {
                        assert!(item_set.replace((block_index, operation_index)).is_none());
                    }
                    _ => {}
                }
            }
        }

        let rhs_get = rhs_get.expect("RHS call target should be loaded once");
        let receiver_get = receiver_get.expect("receiver call target should be loaded once");
        let index_get = index_get.expect("index call target should be loaded once");
        let item_set = item_set.expect("target write should lower to ItemSet");
        assert_eq!(
            call_count, 3,
            "receiver/index/RHS must each be invoked once"
        );
        assert_eq!(rhs_get.0, item_set.0);
        assert_eq!(receiver_get.0, item_set.0);
        assert_eq!(index_get.0, item_set.0);
        assert!(
            rhs_get.1 < receiver_get.1,
            "assignment RHS must run before target receiver"
        );
        assert!(
            receiver_get.1 < index_get.1,
            "receiver must run before target index"
        );
        assert!(index_get.1 < item_set.1);
        assert!(
            function
                .exception_edges
                .contains_key(&(item_set.0 as u32, item_set.1 as u32))
        );
    }

    #[test]
    fn gate4_loop_destructuring_roots_yielded_item_and_iterator_at_unpack_safepoint() {
        let path = Path::new("gate4_loop_target_gc.py");
        let source = r#"
try:
    for first, *rest in [[1, 2, 3], [4, 5]]:
        print(first, rest)
except Exception:
    marker = 1
"#;
        let syntax = syntax::parse(path, source).unwrap();
        let hir = sema::analyze(path, &syntax).unwrap();
        let program = lower(&hir).unwrap();
        let function = &program.functions[program.entry.0 as usize];
        let plan = mir::safepoint_plan(function).unwrap();

        let mut yielded = None;
        let mut iterator = None;
        let mut unpack = None;
        for (block_index, block) in function.blocks.iter().enumerate() {
            for (operation_index, operation) in block.operations.iter().enumerate() {
                match &operation.kind {
                    mir::OperationKind::IteratorNext {
                        item,
                        iterator: source_iterator,
                        ..
                    } => {
                        yielded = Some(*item);
                        iterator = Some(*source_iterator);
                    }
                    mir::OperationKind::Unpack { value, .. } => {
                        unpack = Some((block_index, operation_index, *value));
                    }
                    _ => {}
                }
            }
        }

        let yielded = yielded.expect("for loop should produce an iterator item");
        let iterator = iterator.expect("for loop should keep its iterator");
        let (block, operation, unpack_value) = unpack.expect("starred loop target should unpack");
        assert_eq!(unpack_value, yielded);
        let roots = plan
            .operation_roots(block, operation)
            .expect("unpack must be a safepoint");
        assert!(roots.contains(&yielded));
        assert!(roots.contains(&iterator));
        assert!(
            function
                .exception_edges
                .contains_key(&(block as u32, operation as u32))
        );
    }

    #[test]
    fn gate4_boolean_short_circuit_cfg_roots_selected_values_at_truth_safepoints() {
        let path = Path::new("gate4_boolean_cfg.py");
        let source = r#"
left = [1]
middle = []
right = [3]
selected = left and middle and right
print(selected)
"#;
        let syntax = syntax::parse(path, source).unwrap();
        let hir = sema::analyze(path, &syntax).unwrap();
        let program = lower(&hir).unwrap();
        let function = &program.functions[program.entry.0 as usize];
        let plan = mir::safepoint_plan(function).unwrap();

        let mut branches = 0;
        let mut parameterized_joins = 0;
        for (block_index, block) in function.blocks.iter().enumerate() {
            if !block.parameters.is_empty() {
                parameterized_joins += 1;
            }
            if let mir::Terminator::Branch {
                condition,
                then_arguments,
                else_arguments,
                ..
            } = &block.terminator
            {
                branches += 1;
                let roots = plan
                    .terminator_roots(block_index)
                    .expect("boolean truth test must publish branch roots");
                assert!(roots.contains(condition));
                for value in then_arguments.iter().chain(else_arguments) {
                    assert!(roots.contains(value));
                }
            }
        }
        assert_eq!(
            branches, 2,
            "three operands should lower to two truth branches"
        );
        assert!(
            parameterized_joins >= 2,
            "each short-circuit stage must merge the selected original value"
        );
    }

    #[test]
    fn gate4_comparison_chain_carries_middle_operand_and_exception_edges() {
        let path = Path::new("gate4_compare_chain_mir.py");
        let source = r#"
def value(number):
    return number

try:
    result = value(1) < value(2) < value(3)
except Exception:
    result = False
print(result)
"#;
        let syntax = syntax::parse(path, source).unwrap();
        let hir = sema::analyze(path, &syntax).unwrap();
        let program = lower(&hir).unwrap();
        let function = &program.functions[program.entry.0 as usize];
        let plan = mir::safepoint_plan(function).unwrap();

        let mut compare_locations = Vec::new();
        for (block_index, block) in function.blocks.iter().enumerate() {
            for (operation_index, operation) in block.operations.iter().enumerate() {
                if matches!(operation.kind, mir::OperationKind::Compare { .. }) {
                    compare_locations.push((block_index, operation_index));
                }
            }
        }
        assert_eq!(compare_locations.len(), 2);
        for (block, operation) in &compare_locations {
            assert!(
                function
                    .exception_edges
                    .contains_key(&(*block as u32, *operation as u32)),
                "fallible comparison must retain its try-handler successor"
            );
        }

        let (branch_block, carried, next_target, condition) = function
            .blocks
            .iter()
            .enumerate()
            .find_map(|(block_index, block)| match &block.terminator {
                mir::Terminator::Branch {
                    condition,
                    then_target,
                    then_arguments,
                    else_arguments,
                    ..
                } if then_arguments.len() == 1 && else_arguments.len() == 1 => {
                    Some((block_index, then_arguments[0], *then_target, *condition))
                }
                _ => None,
            })
            .expect("comparison chain should branch while carrying the middle operand");
        let next_left = function.blocks[next_target.0 as usize]
            .parameters
            .first()
            .copied()
            .expect("comparison continuation must receive the middle operand as a block parameter");
        let roots = plan
            .terminator_roots(branch_block)
            .expect("comparison truth test must publish roots");
        assert!(roots.contains(&condition));
        assert!(roots.contains(&carried));
        assert!(
            function.blocks[next_target.0 as usize]
                .operations
                .iter()
                .any(|operation| matches!(
                    operation.kind,
                    mir::OperationKind::Compare { left, .. } if left == next_left
                ))
        );
    }

    #[test]
    fn gate4_extended_subscript_roots_receiver_tuple_components_and_rhs() {
        let path = Path::new("gate4_extended_subscript_mir.py");
        let source = r#"
items = {}
right = 7
try:
    items[1, 2:5:2] = right
except Exception:
    marker = 1
"#;
        let syntax = syntax::parse(path, source).unwrap();
        let hir = sema::analyze(path, &syntax).unwrap();
        let program = lower(&hir).unwrap();
        let function = &program.functions[program.entry.0 as usize];
        let plan = mir::safepoint_plan(function).unwrap();

        let mut slice_dest = None;
        let mut tuple = None;
        let mut item_set = None;
        for (block_index, block) in function.blocks.iter().enumerate() {
            for (operation_index, operation) in block.operations.iter().enumerate() {
                match &operation.kind {
                    mir::OperationKind::SliceNew {
                        dest,
                        start,
                        stop,
                        step,
                    } => {
                        assert!(start.is_some() && stop.is_some() && step.is_some());
                        slice_dest = Some(*dest);
                    }
                    mir::OperationKind::Tuple { dest, values } if values.len() == 2 => {
                        tuple = Some((block_index, operation_index, *dest, values.clone()));
                    }
                    mir::OperationKind::ItemSet {
                        collection,
                        index,
                        value,
                    } => {
                        item_set =
                            Some((block_index, operation_index, *collection, *index, *value));
                    }
                    _ => {}
                }
            }
        }
        let slice_dest = slice_dest.expect("extended subscript should construct a native slice");
        let (tuple_block, tuple_operation, tuple_dest, tuple_values) =
            tuple.expect("extended subscript should materialize one tuple index");
        assert!(tuple_values.contains(&slice_dest));
        let tuple_roots = plan
            .operation_roots(tuple_block, tuple_operation)
            .expect("tuple construction is a safepoint");
        for component in &tuple_values {
            assert!(tuple_roots.contains(component));
        }

        let (set_block, set_operation, collection, index, value) =
            item_set.expect("extended assignment should lower to ItemSet");
        assert_eq!(index, tuple_dest);
        let set_roots = plan
            .operation_roots(set_block, set_operation)
            .expect("item assignment is a safepoint");
        assert!(set_roots.contains(&collection));
        assert!(set_roots.contains(&index));
        assert!(set_roots.contains(&value));
        assert!(
            function
                .exception_edges
                .contains_key(&(set_block as u32, set_operation as u32))
        );
    }

    #[test]
    fn gate4_slice11_mir_pins_inplace_store_delete_and_lazy_assert_failure() {
        let path = Path::new("gate4_slice11_mir.py");
        let source = r#"
items = [1, 2]

def message():
    return "boom"

try:
    items[0] += 4
    del items[1]
    assert items, message()
except Exception:
    marker = 1
"#;
        let syntax = syntax::parse(path, source).unwrap();
        let hir = sema::analyze(path, &syntax).unwrap();
        let program = lower(&hir).unwrap();
        let function = &program.functions[program.entry.0 as usize];

        let mut inplace = None;
        let mut item_set = None;
        let mut item_delete = None;
        let mut assertion_branch = None;
        for (block_index, block) in function.blocks.iter().enumerate() {
            for (operation_index, operation) in block.operations.iter().enumerate() {
                match &operation.kind {
                    mir::OperationKind::InPlace { dest, .. } => {
                        inplace = Some((block_index, operation_index, *dest));
                    }
                    mir::OperationKind::ItemSet { value, .. } => {
                        item_set = Some((block_index, operation_index, *value));
                    }
                    mir::OperationKind::ItemDelete { .. } => {
                        item_delete = Some((block_index, operation_index));
                    }
                    _ => {}
                }
            }
            if let mir::Terminator::Branch {
                else_target,
                condition,
                ..
            } = block.terminator
                && function.blocks[else_target.0 as usize]
                    .operations
                    .iter()
                    .any(|operation| {
                        matches!(
                            &operation.kind,
                            mir::OperationKind::GlobalGet { name, .. } if name == "AssertionError"
                        )
                    })
            {
                assertion_branch = Some((block_index, condition, else_target));
            }
        }

        let (inplace_block, inplace_operation, inplace_result) =
            inplace.expect("augmented item assignment should emit InPlace");
        let (set_block, set_operation, set_value) =
            item_set.expect("augmented item assignment should store its inplace result");
        assert_eq!(inplace_result, set_value);
        for (block, operation) in [
            (inplace_block, inplace_operation),
            (set_block, set_operation),
            item_delete.expect("delete should emit ItemDelete"),
        ] {
            assert!(
                function
                    .exception_edges
                    .contains_key(&(block as u32, operation as u32)),
                "Slice 11 fallible statement operation must keep its try successor"
            );
        }

        let (branch_block, condition, failed) =
            assertion_branch.expect("assert must branch to a dedicated lazy failure block");
        let failed_block = &function.blocks[failed.0 as usize];
        assert!(failed_block.operations.iter().any(|operation| matches!(
            &operation.kind,
            mir::OperationKind::GlobalGet { name, .. } if name == "message"
        )));
        assert!(failed_block.operations.iter().any(|operation| matches!(
            &operation.kind,
            mir::OperationKind::GlobalGet { name, .. } if name == "AssertionError"
        )));
        assert!(
            failed_block
                .operations
                .iter()
                .any(|operation| matches!(operation.kind, mir::OperationKind::Raise { .. }))
        );
        assert!(
            mir::safepoint_plan(function)
                .unwrap()
                .terminator_roots(branch_block)
                .expect("assert truth branch must publish roots")
                .contains(&condition)
        );
    }

    #[test]
    fn gate4_named_expression_stores_once_and_returns_the_identical_mir_value() {
        let path = Path::new("gate4_named_expression_mir.py");
        let source = r#"
def make():
    return [1]

try:
    result = (named := make())
except Exception:
    result = None
print(result is named)
"#;
        let syntax = syntax::parse(path, source).unwrap();
        let hir = sema::analyze(path, &syntax).unwrap();
        let program = lower(&hir).unwrap();
        let function = &program.functions[program.entry.0 as usize];

        let mut stored = None;
        let mut returned_copy = None;
        let mut make_call = None;
        for (block_index, block) in function.blocks.iter().enumerate() {
            for (operation_index, operation) in block.operations.iter().enumerate() {
                match &operation.kind {
                    mir::OperationKind::GlobalSet { name, value } if name == "named" => {
                        stored = Some(*value);
                    }
                    mir::OperationKind::Copy { dest, source } if Some(*source) == stored => {
                        returned_copy = Some((*dest, *source));
                    }
                    mir::OperationKind::Call { dest, .. } if make_call.is_none() => {
                        make_call = Some((block_index, operation_index, *dest));
                    }
                    _ => {}
                }
            }
        }
        let stored = stored.expect("walrus target must use the normal global store");
        let (copy_dest, copy_source) = returned_copy.expect("walrus must return the stored value");
        assert_eq!(copy_source, stored);
        assert_ne!(
            copy_dest, stored,
            "MIR may copy the slot but not recompute the object"
        );
        let (block, operation, call_result) =
            make_call.expect("walrus RHS call should be emitted once");
        assert_eq!(
            call_result, stored,
            "the call result itself must be stored by :="
        );
        assert!(
            function
                .exception_edges
                .contains_key(&(block as u32, operation as u32))
        );
    }

    #[test]
    fn gate4_annotation_mir_initializes_namespaces_and_skips_local_annotation_evaluation() {
        let path = Path::new("gate4_annotations_mir.py");
        let source = r#"
def mark(value):
    return value

x: mark(int) = 1

def local():
    hidden: mark(int)
    return 1

class C:
    y: mark(int) = 2
"#;
        let syntax = syntax::parse(path, source).unwrap();
        let hir = sema::analyze(path, &syntax).unwrap();
        let program = lower(&hir).unwrap();
        let module = &program.functions[program.entry.0 as usize];
        assert!(module.blocks[0].operations.iter().any(|operation| matches!(
            operation.kind,
            mir::OperationKind::AnnotationsEnsure { namespace: None }
        )));

        let local = program
            .functions
            .iter()
            .find(|function| function.name == "local")
            .expect("local function should be lowered");
        assert!(!local.blocks.iter().flat_map(|block| &block.operations).any(|operation| {
            matches!(&operation.kind, mir::OperationKind::GlobalGet { name, .. } if name == "mark")
        }), "function-local variable annotations must not evaluate");

        let class_body = program
            .functions
            .iter()
            .find(|function| {
                function.kind == mir::FunctionKind::ClassBody && function.name == "<class body C>"
            })
            .expect("class body should be lowered");
        let (block_index, operation_index, namespace) = class_body
            .blocks
            .iter()
            .enumerate()
            .find_map(|(block_index, block)| {
                block
                    .operations
                    .iter()
                    .enumerate()
                    .find_map(|(operation_index, operation)| match operation.kind {
                        mir::OperationKind::AnnotationsEnsure {
                            namespace: Some(namespace),
                        } => Some((block_index, operation_index, namespace)),
                        _ => None,
                    })
            })
            .expect("class body must ensure __annotations__ in its prepared namespace");
        let plan = mir::safepoint_plan(class_body).unwrap();
        let roots = plan
            .operation_roots(block_index, operation_index)
            .expect("annotation namespace setup is a safepoint");
        assert!(roots.contains(&namespace));
    }

    #[test]
    fn gate4_fstring_mir_roots_earlier_segments_across_later_format_callbacks() {
        let path = Path::new("gate4_fstring_mir.py");
        let source = r#"
def left():
    return "left"
def right():
    return 7
def width():
    return 4
result = f"{left()}:{right():>{width()}}"
print(result)
"#;
        let syntax = syntax::parse(path, source).unwrap();
        let hir = sema::analyze(path, &syntax).unwrap();
        let program = lower(&hir).unwrap();
        let function = &program.functions[program.entry.0 as usize];
        let plan = mir::safepoint_plan(function).unwrap();

        let mut formatted = Vec::new();
        for (block_index, block) in function.blocks.iter().enumerate() {
            for (operation_index, operation) in block.operations.iter().enumerate() {
                if let mir::OperationKind::FormatValue {
                    dest, value, spec, ..
                } = operation.kind
                {
                    formatted.push((block_index, operation_index, dest, value, spec));
                }
            }
        }
        assert!(
            formatted.len() >= 3,
            "nested width and two outer fields must all use format_value"
        );
        let first_outer = formatted[0].2;
        let &(block, operation, _dest, value, spec) = formatted.last().unwrap();
        let prefix = function.blocks[block]
            .operations
            .iter()
            .take(operation)
            .find_map(|operation| match operation.kind {
                mir::OperationKind::Binary {
                    dest,
                    op: mir::BinaryOperator::Add,
                    left,
                    ..
                } if left == first_outer => Some(dest),
                _ => None,
            })
            .expect("later field should format after the earlier field was accumulated");
        let roots = plan
            .operation_roots(block, operation)
            .expect("format callback must be a safepoint");
        assert!(roots.contains(&value));
        assert!(roots.contains(&spec));
        assert!(
            roots.contains(&prefix),
            "the accumulated earlier f-string prefix must stay rooted during later formatting"
        );
    }

    #[test]
    fn gate4_comprehension_cfg_uses_hidden_activation_and_roots_live_loop_state() {
        let path = Path::new("gate4_comprehension_mir.py");
        let source = r#"
try:
    result = [left + right for left in [1, 2] if left for right in [3, 4]]
except Exception:
    result = []
print(result)
"#;
        let syntax = syntax::parse(path, source).unwrap();
        let hir = sema::analyze(path, &syntax).unwrap();
        let program = lower(&hir).unwrap();
        let module = &program.functions[program.entry.0 as usize];
        assert!(
            module
                .blocks
                .iter()
                .flat_map(|block| &block.operations)
                .any(|operation| {
                    matches!(operation.kind, mir::OperationKind::IteratorNew { .. })
                })
        );
        let (call_block, call_operation) = module
            .blocks
            .iter()
            .enumerate()
            .find_map(|(block_index, block)| {
                block
                    .operations
                    .iter()
                    .enumerate()
                    .find_map(|(operation_index, operation)| {
                        matches!(operation.kind, mir::OperationKind::Call { .. })
                            .then_some((block_index, operation_index))
                    })
            })
            .expect("caller must invoke a hidden comprehension function");
        assert!(
            module
                .exception_edges
                .contains_key(&(call_block as u32, call_operation as u32))
        );

        let hidden = program
            .functions
            .iter()
            .find(|function| function.name == "<listcomp>")
            .expect("list comprehension must lower to a hidden function");
        assert_eq!(hidden.parameters.len(), 1);
        assert_eq!(hidden.parameters[0].name, ".0");
        let plan = mir::safepoint_plan(hidden).unwrap();
        let mut append = None;
        let mut iterators = Vec::new();
        for (block_index, block) in hidden.blocks.iter().enumerate() {
            for (operation_index, operation) in block.operations.iter().enumerate() {
                match operation.kind {
                    mir::OperationKind::IteratorNext { iterator, .. } => iterators.push(iterator),
                    mir::OperationKind::ListAppend { list, value } => {
                        append = Some((block_index, operation_index, list, value));
                    }
                    _ => {}
                }
            }
        }
        assert_eq!(
            iterators.len(),
            2,
            "nested clauses need two iterator regions"
        );
        let (block, operation, list, value) =
            append.expect("list sink must append in hidden scope");
        let roots = plan
            .operation_roots(block, operation)
            .expect("list append is a runtime safepoint");
        assert!(roots.contains(&list));
        assert!(roots.contains(&value));
        assert!(iterators.iter().any(|iterator| roots.contains(iterator)));
    }

    #[test]
    fn gate5_function_mir_uses_one_make_function_path_for_metadata_decorators_and_closures() {
        let path = Path::new("gate5_function_mir.py");
        let source = r#"
events = []

def decorate(function):
    return function

@decorate
def outer(value: int = 3, *, flag: int = 4) -> int:
    captured = value
    def inner():
        return captured
    return inner()
"#;
        let syntax = syntax::parse(path, source).unwrap();
        let hir = sema::analyze(path, &syntax).unwrap();
        let program = lower(&hir).unwrap();
        mir::verify(&program).unwrap();

        let module = &program.functions[program.entry.0 as usize];
        let outer_id = program
            .functions
            .iter()
            .position(|function| function.name == "outer")
            .expect("outer function should exist");
        let mut saw_outer_make = false;
        let mut saw_annotation_publish = false;
        let mut saw_decorator_call = false;
        for block in &module.blocks {
            for operation in &block.operations {
                match &operation.kind {
                    mir::OperationKind::MakeFunction {
                        function, defaults, ..
                    } if function.0 as usize == outer_id => {
                        saw_outer_make = true;
                        assert_eq!(defaults.len(), 2);
                    }
                    mir::OperationKind::AttributeSet { name, .. } if name == "__annotations__" => {
                        saw_annotation_publish = true;
                    }
                    mir::OperationKind::Call { .. } if saw_outer_make => {
                        saw_decorator_call = true;
                    }
                    _ => {}
                }
            }
        }
        assert!(saw_outer_make);
        assert!(saw_annotation_publish);
        assert!(saw_decorator_call);

        let outer = &program.functions[outer_id];
        let inner_id = program
            .functions
            .iter()
            .position(|function| function.qualified_name == "outer.<locals>.inner")
            .expect("nested qualified name should be materialized in MIR");
        assert!(
            outer
                .blocks
                .iter()
                .flat_map(|block| &block.operations)
                .any(|operation| {
                    matches!(
                        operation.kind,
                        mir::OperationKind::MakeFunction { function, ref closure, .. }
                            if function.0 as usize == inner_id && closure.len() == 1
                    )
                })
        );
    }

    #[test]
    fn gate4_generator_expression_mir_uses_yield_and_persists_only_live_state() {
        let path = Path::new("gate4_generator_mir.py");
        let source = r#"
def make():
    prefix = "p"
    generator = (prefix + str(left + right) for left in [1, 2] for right in [10, 20])
    prefix = "q"
    return generator

print(list(make()))
"#;
        let syntax = syntax::parse(path, source).unwrap();
        let hir = sema::analyze(path, &syntax).unwrap();
        let program = lower(&hir).unwrap();
        mir::verify(&program).unwrap();
        let generator = program
            .functions
            .iter()
            .find(|function| function.name == "<genexpr>")
            .expect("generator expression must lower to a hidden generator function");
        assert_eq!(generator.kind, mir::FunctionKind::Generator);
        assert_eq!(generator.parameters.len(), 1);
        assert_eq!(generator.parameters[0].name, ".0");

        let persistent = mir::generator_persistent_values(generator).unwrap();
        assert!(persistent.contains(&generator.parameters[0].value));
        assert!(persistent.len() < generator.value_count as usize);

        let inner_iterator = generator
            .blocks
            .iter()
            .flat_map(|block| &block.operations)
            .find_map(|operation| match operation.kind {
                mir::OperationKind::IteratorNew { dest, .. } => Some(dest),
                _ => None,
            })
            .expect("nested generator clause must create a lazy inner iterator");
        assert!(
            persistent.contains(&inner_iterator),
            "inner iterator must survive suspension"
        );

        let plan = mir::safepoint_plan(generator).unwrap();
        let mut yields = 0;
        for (block_index, block) in generator.blocks.iter().enumerate() {
            if let mir::Terminator::Yield { value, .. } = block.terminator {
                yields += 1;
                let roots = plan
                    .terminator_roots(block_index)
                    .expect("yield must publish a suspension root set");
                assert!(roots.contains(&value));
                assert!(roots.contains(&inner_iterator));
                assert!(
                    !persistent.contains(&value),
                    "the yielded temporary is dead after suspension and must not consume a persistent slot"
                );
            }
        }
        assert_eq!(
            yields, 1,
            "one comprehension sink should lower to one yield site"
        );
    }

    #[test]
    fn gate4_match_cfg_commits_only_on_success_and_roots_subject_across_callbacks() {
        let path = Path::new("gate4_match_mir.py");
        let source = r#"
class Values:
    first = "one"
    second = "two"

def subject():
    return "one"

def guard(value):
    return value

match subject():
    case Values.first as captured if guard(captured):
        result = captured
    case Values.second:
        result = "second"
    case _:
        result = "fallback"
"#;
        let syntax = syntax::parse(path, source).unwrap();
        let hir = sema::analyze(path, &syntax).unwrap();
        let program = lower(&hir).unwrap();
        mir::verify(&program).unwrap();
        let function = &program.functions[program.entry.0 as usize];
        let plan = mir::safepoint_plan(function).unwrap();

        let mut subject_get = None;
        let mut subject_get_count = 0;
        let mut captured_store = None;
        for (block_index, block) in function.blocks.iter().enumerate() {
            for (operation_index, operation) in block.operations.iter().enumerate() {
                match &operation.kind {
                    mir::OperationKind::GlobalGet { dest, name } if name == "subject" => {
                        subject_get_count += 1;
                        subject_get = Some(*dest);
                    }
                    mir::OperationKind::GlobalSet { name, value } if name == "captured" => {
                        captured_store = Some((block_index, operation_index, *value));
                    }
                    _ => {}
                }
            }
        }
        assert_eq!(
            subject_get_count, 1,
            "match subject callable must be resolved once"
        );
        let subject_callable = subject_get.expect("match subject callable must be loaded");
        let subject = function
            .blocks
            .iter()
            .flat_map(|block| &block.operations)
            .find_map(|operation| match &operation.kind {
                mir::OperationKind::Call { dest, callable, .. }
                    if *callable == subject_callable =>
                {
                    Some(*dest)
                }
                _ => None,
            })
            .expect("match subject must be evaluated exactly once");
        let (captured_block, captured_index, captured_value) =
            captured_store.expect("successful capture must commit a binding");
        let tentative = function
            .blocks
            .iter()
            .flat_map(|block| &block.operations)
            .find_map(|operation| match operation.kind {
                mir::OperationKind::DictionaryInsert {
                    dictionary, value, ..
                } if value == subject => Some(dictionary),
                _ => None,
            })
            .expect("successful AS capture must first publish into tentative storage");
        assert!(
            function.blocks[captured_block]
                .operations
                .iter()
                .any(|operation| {
                    matches!(
                        operation.kind,
                        mir::OperationKind::ItemGet {
                            dest,
                            collection,
                            ..
                        } if dest == captured_value && collection == tentative
                    )
                })
        );

        let mut first_compare = None;
        for (block_index, block) in function.blocks.iter().enumerate() {
            for (operation_index, operation) in block.operations.iter().enumerate() {
                if let mir::OperationKind::Compare { dest, left, .. } = operation.kind
                    && left == subject
                {
                    let roots = plan
                        .operation_roots(block_index, operation_index)
                        .expect("pattern equality is a runtime safepoint");
                    assert!(roots.contains(&subject));
                    if first_compare.is_none() {
                        first_compare = Some((block_index, dest));
                    }
                }
            }
        }

        let (compare_block, compare_result) =
            first_compare.expect("value pattern must compare against the subject");
        let mir::Terminator::Branch {
            condition,
            then_target,
            else_target,
            ..
        } = function.blocks[compare_block].terminator
        else {
            panic!("value-pattern comparison must branch to success/failure CFG");
        };
        assert_eq!(condition, compare_result);
        assert!(
            function.blocks[then_target.0 as usize]
                .operations
                .iter()
                .any(|operation| matches!(
                    operation.kind,
                    mir::OperationKind::DictionaryInsert { dictionary, value, .. }
                        if dictionary == tentative && value == subject
                ))
        );
        assert!(
            !function.blocks[else_target.0 as usize]
                .operations
                .iter()
                .any(|operation| matches!(
                    &operation.kind,
                    mir::OperationKind::GlobalSet { name, .. } if name == "captured"
                )),
            "pattern failure must not publish tentative captures"
        );

        let matched_operations = &function.blocks[captured_block].operations;
        let guard_get = matched_operations
            .iter()
            .position(|operation| {
                matches!(
                    &operation.kind,
                    mir::OperationKind::GlobalGet { name, .. } if name == "guard"
                )
            })
            .expect("guard callable must be loaded after capture commit");
        let guard_call = matched_operations
            .iter()
            .enumerate()
            .skip(guard_get + 1)
            .find_map(|(index, operation)| {
                matches!(operation.kind, mir::OperationKind::Call { .. }).then_some(index)
            })
            .expect("guard must call through ordinary call MIR");
        assert!(captured_index < guard_get);
        assert!(guard_get < guard_call);
        let guard_roots = plan
            .operation_roots(captured_block, guard_call)
            .expect("guard call must publish live roots");
        assert!(
            guard_roots.contains(&subject),
            "subject must remain rooted when a false guard continues matching later cases"
        );
    }

    #[test]
    fn gate4_structural_pattern_extractors_publish_exact_safepoint_roots() {
        let path = Path::new("gate4_structural_patterns_mir.py");
        let source = r#"
subject = {"items": [1, 2, 3], "keep": 4}
try:
    match subject:
        case {"items": [first, *middle], "keep": kept, **rest}:
            result = (first, middle, kept, rest)
        case _:
            result = None
except Exception:
    result = None
"#;
        let syntax = syntax::parse(path, source).unwrap();
        let hir = sema::analyze(path, &syntax).unwrap();
        let program = lower(&hir).unwrap();
        let function = &program.functions[program.entry.0 as usize];
        let plan = mir::safepoint_plan(function).unwrap();

        let mut saw_sequence = false;
        let mut saw_mapping_check = false;
        let mut saw_mapping = false;
        for (block_index, block) in function.blocks.iter().enumerate() {
            for (operation_index, operation) in block.operations.iter().enumerate() {
                match &operation.kind {
                    mir::OperationKind::PatternSequence { subject, .. } => {
                        saw_sequence = true;
                        let roots = plan.operation_roots(block_index, operation_index).unwrap();
                        assert!(roots.contains(subject));
                        assert!(
                            function
                                .exception_edges
                                .contains_key(&(block_index as u32, operation_index as u32))
                        );
                    }
                    mir::OperationKind::PatternMappingCheck { subject, .. } => {
                        saw_mapping_check = true;
                        let roots = plan.operation_roots(block_index, operation_index).unwrap();
                        assert!(roots.contains(subject));
                        assert!(
                            function
                                .exception_edges
                                .contains_key(&(block_index as u32, operation_index as u32))
                        );
                    }
                    mir::OperationKind::PatternMapping { subject, keys, .. } => {
                        saw_mapping = true;
                        let roots = plan.operation_roots(block_index, operation_index).unwrap();
                        assert!(roots.contains(subject));
                        for key in keys {
                            assert!(roots.contains(key));
                        }
                        assert!(
                            function
                                .exception_edges
                                .contains_key(&(block_index as u32, operation_index as u32))
                        );
                    }
                    _ => {}
                }
            }
        }
        assert!(saw_sequence && saw_mapping_check && saw_mapping);
    }

    #[test]
    fn gate4_class_pattern_roots_subject_and_class_and_evaluates_class_expression_once() {
        let path = Path::new("gate4_class_pattern_mir.py");
        let source = r#"
class Point:
    __match_args__ = ("x", "y")
    def __init__(self, x, y):
        self.x = x
        self.y = y
class Holder:
    target = Point
subject = Point(1, 2)
try:
    match subject:
        case Holder.target(left, right):
            result = left + right
        case _:
            result = 0
except Exception:
    result = -1
"#;
        let syntax = syntax::parse(path, source).unwrap();
        let hir = sema::analyze(path, &syntax).unwrap();
        let program = lower(&hir).unwrap();
        let function = &program.functions[program.entry.0 as usize];
        let plan = mir::safepoint_plan(function).unwrap();

        let class_gets = function
            .blocks
            .iter()
            .flat_map(|block| &block.operations)
            .filter_map(|operation| match &operation.kind {
                mir::OperationKind::AttributeGet { dest, name, .. } if name == "target" => {
                    Some(*dest)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            class_gets.len(),
            1,
            "class expression must be evaluated once"
        );

        let mut saw_class = false;
        for (block_index, block) in function.blocks.iter().enumerate() {
            for (operation_index, operation) in block.operations.iter().enumerate() {
                if let mir::OperationKind::PatternClass {
                    subject,
                    class,
                    positional_count,
                    keyword_names,
                    ..
                } = &operation.kind
                {
                    saw_class = true;
                    assert_eq!(*class, class_gets[0]);
                    assert_eq!(*positional_count, 2);
                    assert!(keyword_names.is_empty());
                    let roots = plan.operation_roots(block_index, operation_index).unwrap();
                    assert!(roots.contains(subject));
                    assert!(roots.contains(class));
                    assert!(
                        function
                            .exception_edges
                            .contains_key(&(block_index as u32, operation_index as u32))
                    );
                }
            }
        }
        assert!(saw_class);
    }

    #[test]
    fn gate4_set_and_dict_comprehension_sinks_root_inputs_and_dict_evaluates_key_first() {
        let path = Path::new("gate4_set_dict_comprehension_mir.py");
        let source = r#"
def key(value):
    return value
def mapped(value):
    return value
sets = {item for item in [1, 2]}
dicts = {key(item): mapped(item) for item in [1, 2]}
"#;
        let syntax = syntax::parse(path, source).unwrap();
        let hir = sema::analyze(path, &syntax).unwrap();
        let program = lower(&hir).unwrap();

        let set_comp = program
            .functions
            .iter()
            .find(|function| function.name == "<setcomp>")
            .expect("set comprehension function");
        let set_plan = mir::safepoint_plan(set_comp).unwrap();
        let (block, operation, set, value) = set_comp
            .blocks
            .iter()
            .enumerate()
            .find_map(|(block_index, block)| {
                block
                    .operations
                    .iter()
                    .enumerate()
                    .find_map(|(operation_index, operation)| match operation.kind {
                        mir::OperationKind::SetInsert { set, value } => {
                            Some((block_index, operation_index, set, value))
                        }
                        _ => None,
                    })
            })
            .expect("set sink");
        let roots = set_plan.operation_roots(block, operation).unwrap();
        assert!(roots.contains(&set));
        assert!(roots.contains(&value));

        let dict_comp = program
            .functions
            .iter()
            .find(|function| function.name == "<dictcomp>")
            .expect("dict comprehension function");
        let dict_plan = mir::safepoint_plan(dict_comp).unwrap();
        let mut key_get = None;
        let mut value_get = None;
        let mut insert = None;
        for (block_index, block) in dict_comp.blocks.iter().enumerate() {
            for (operation_index, operation) in block.operations.iter().enumerate() {
                match &operation.kind {
                    mir::OperationKind::GlobalGet { name, .. } if name == "key" => {
                        key_get = Some((block_index, operation_index));
                    }
                    mir::OperationKind::GlobalGet { name, .. } if name == "mapped" => {
                        value_get = Some((block_index, operation_index));
                    }
                    mir::OperationKind::DictionaryInsert {
                        dictionary,
                        key,
                        value,
                    } => insert = Some((block_index, operation_index, *dictionary, *key, *value)),
                    _ => {}
                }
            }
        }
        let key_get = key_get.expect("dict key callback load");
        let value_get = value_get.expect("dict value callback load");
        assert_eq!(key_get.0, value_get.0);
        assert!(
            key_get.1 < value_get.1,
            "dictionary keys must evaluate before values"
        );
        let (block, operation, dictionary, key, value) = insert.expect("dict sink");
        let roots = dict_plan.operation_roots(block, operation).unwrap();
        assert!(roots.contains(&dictionary));
        assert!(roots.contains(&key));
        assert!(roots.contains(&value));
    }

    #[test]
    fn gate7_namespace_reflection_mir_declares_authoritative_scope_ownership() {
        let path = Path::new("gate7_namespace_reflection_mir.py");
        let source = r#"
def outer(z, a):
    q = 1
    values = [locals()["i"] for i in [1]]
    generated = (locals()["i"] for i in [2])
    return q, values, generated

class C:
    marker = 1
    here = locals()
"#;
        let syntax = syntax::parse(path, source).unwrap();
        let hir = sema::analyze(path, &syntax).unwrap();
        let program = lower(&hir).unwrap();
        mir::verify(&program).unwrap();

        let outer = program
            .functions
            .iter()
            .find(|function| function.name == "outer")
            .expect("outer function");
        assert!(
            outer.blocks[outer.entry.0 as usize]
                .operations
                .iter()
                .any(|operation| {
                    matches!(
                        operation.kind,
                        mir::OperationKind::ReflectionScopeConfigure {
                            namespace: None,
                            comprehension: false,
                        }
                    )
                })
        );
        let registered = outer.blocks[outer.entry.0 as usize]
            .operations
            .iter()
            .filter_map(|operation| match &operation.kind {
                mir::OperationKind::ReflectionLocalRegister { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(registered, ["z", "a", "q", "values", "generated"]);

        let list_comp = program
            .functions
            .iter()
            .find(|function| function.name == "<listcomp>")
            .expect("list comprehension");
        assert!(
            list_comp.blocks[list_comp.entry.0 as usize]
                .operations
                .iter()
                .any(|operation| {
                    matches!(
                        operation.kind,
                        mir::OperationKind::ReflectionScopeConfigure {
                            namespace: None,
                            comprehension: true,
                        }
                    )
                })
        );
        assert!(
            list_comp.blocks[list_comp.entry.0 as usize]
                .operations
                .iter()
                .any(|operation| {
                    matches!(
                        &operation.kind,
                        mir::OperationKind::ReflectionLocalRegister { name, .. } if name == "i"
                    )
                })
        );

        let generator = program
            .functions
            .iter()
            .find(|function| function.name == "<genexpr>")
            .expect("generator expression");
        let generator_ops = &generator.blocks[generator.entry.0 as usize].operations;
        assert!(generator_ops.iter().any(|operation| {
            matches!(
                operation.kind,
                mir::OperationKind::ReflectionScopeConfigure {
                    namespace: None,
                    comprehension: false,
                }
            )
        }));
        let generator_names = generator_ops
            .iter()
            .filter_map(|operation| match &operation.kind {
                mir::OperationKind::ReflectionLocalRegister { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(generator_names, [".0", "i"]);

        let class_body = program
            .functions
            .iter()
            .find(|function| function.kind == mir::FunctionKind::ClassBody)
            .expect("class body");
        assert!(
            class_body.blocks[class_body.entry.0 as usize]
                .operations
                .iter()
                .any(|operation| {
                    matches!(
                        operation.kind,
                        mir::OperationKind::ReflectionScopeConfigure {
                            namespace: Some(_),
                            comprehension: false,
                        }
                    )
                })
        );
    }
}
