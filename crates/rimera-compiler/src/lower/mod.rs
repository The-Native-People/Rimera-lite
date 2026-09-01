use std::collections::BTreeMap;

use crate::core::Span;
use crate::{hir, mir};

// Module chunks are internal execution units. Keeping their source suites
// bounded prevents target backends from imposing a whole-module code-size
// ceiling while preserving statement boundaries and module-global semantics.
const MODULE_CHUNK_STATEMENTS: usize = 512;

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
        program.lower_module_driver(&chunks)?
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

struct ProgramLowerer {
    functions: Vec<Option<mir::Function>>,
}

impl ProgramLowerer {
    fn lower_class_body(
        &mut self,
        name: String,
        qualified_name: String,
        body: &[hir::ClassMember],
        free: &[String],
    ) -> Result<mir::FunctionId, String> {
        let id = mir::FunctionId(
            u32::try_from(self.functions.len()).map_err(|_| "too many MIR functions")?,
        );
        self.functions.push(None);
        let mut lowerer = Lowerer::new(self);
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
        lowerer.class_scopes.push(ClassScope { namespace });
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
    ) -> Result<mir::FunctionId, String> {
        let id = mir::FunctionId(
            u32::try_from(self.functions.len()).map_err(|_| "too many MIR functions")?,
        );
        self.functions.push(None);
        let mut lowerer = Lowerer::new(self);
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
        let mut lowerer = Lowerer::new(self);
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
        if !module_scope {
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
        namespace: mir::ValueId,
        binding: Option<String>,
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
            hir::StatementKind::FunctionDef {
                name,
                binding,
                decorators: _,
                parameters,
                body,
                locals,
                cells: _,
                free,
            } => {
                let qualified_name = name.clone();
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
                    statement.span,
                    mir::OperationKind::MakeFunction {
                        dest: value,
                        function,
                        defaults,
                        closure,
                    },
                );
                self.store_name(statement.span, name, *binding, value)?;
            }
            hir::StatementKind::ClassDef {
                name,
                binding,
                decorators,
                bases,
                metaclass,
                keywords,
                body,
            } => {
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
                // A class suite is a real native function.  Its namespace is
                // supplied only after `__prepare__`, so every class-local
                // access is scoped to the mapping selected by the metaclass.
                let class_free = self.cells.keys().cloned().collect::<Vec<_>>();
                let class_body = self.program.lower_class_body(
                    format!("<class body {name}>"),
                    format!("{name}.<class body>"),
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
                self.class_scopes.push(ClassScope { namespace });
                let has_class_cell = class_members_need_class_cell(body);
                // The suite above is the executable class body. Keep this
                // legacy lowering structure inert while its helpers remain
                // shared by the hidden class-body lowerer.
                for member in body.iter().take(0) {
                    match member {
                        hir::ClassMember::Assign { name, value } => {
                            let value = self.expression(value)?;
                            self.emit(
                                statement.span,
                                mir::OperationKind::ClassNamespaceSet {
                                    namespace,
                                    name: name.clone(),
                                    value,
                                },
                            );
                            self.record_class_name(name);
                        }
                        hir::ClassMember::AugAssign { name, op, value } => {
                            self.lower_class_augmented_assignment(
                                statement.span,
                                namespace,
                                name,
                                *op,
                                value,
                            )?;
                        }
                        hir::ClassMember::Delete { name } => self.emit(
                            statement.span,
                            mir::OperationKind::ClassNamespaceDelete {
                                namespace,
                                name: name.clone(),
                            },
                        ),
                        hir::ClassMember::ItemAssign {
                            collection,
                            index,
                            value,
                        } => {
                            let collection = self.expression(collection)?;
                            let index = self.expression(index)?;
                            let value = self.expression(value)?;
                            self.emit(
                                statement.span,
                                mir::OperationKind::ItemSet {
                                    collection,
                                    index,
                                    value,
                                },
                            );
                        }
                        hir::ClassMember::AttributeAssign {
                            receiver,
                            name,
                            value,
                        } => {
                            let receiver = self.expression(receiver)?;
                            let value = self.expression(value)?;
                            self.emit(
                                statement.span,
                                mir::OperationKind::AttributeSet {
                                    receiver,
                                    name: name.clone(),
                                    value,
                                },
                            );
                        }
                        hir::ClassMember::AttributeDelete { receiver, name } => {
                            let receiver = self.expression(receiver)?;
                            self.emit(
                                statement.span,
                                mir::OperationKind::AttributeDelete {
                                    receiver,
                                    name: name.clone(),
                                },
                            );
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
                        hir::ClassMember::Try {
                            body,
                            handlers,
                            else_body,
                            finally_body,
                            is_star,
                        } => self.lower_class_try_members(
                            statement.span,
                            namespace,
                            body,
                            handlers,
                            else_body,
                            finally_body,
                            *is_star,
                        )?,
                        hir::ClassMember::ClassDef {
                            name,
                            decorators,
                            bases,
                            metaclass,
                            keywords,
                            body,
                        } => self.lower_nested_class_member(
                            statement.span,
                            namespace,
                            name,
                            decorators,
                            bases,
                            metaclass,
                            keywords,
                            body,
                        )?,
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
                        hir::ClassMember::FunctionDef {
                            name: method_name,
                            decorators,
                            uses_zero_argument_super,
                            parameters,
                            body,
                            locals,
                            cells: _,
                            free,
                        } => {
                            let mut method_free = free.clone();
                            if *uses_zero_argument_super {
                                method_free.push("__class__".to_owned());
                            }
                            let function = self.program.lower_scope(
                                method_name.clone(),
                                format!("{name}.{method_name}"),
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
                                        u32::try_from(index)
                                            .map_err(|_| "too many method parameters")?,
                                        self.expression(default)?,
                                    ));
                                }
                            }
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
                                statement.span,
                                mir::OperationKind::MakeFunction {
                                    dest: method,
                                    function,
                                    defaults,
                                    closure,
                                },
                            );
                            self.record_class_name(method_name);
                            let mut decorated = method;
                            for decorator in decorators.iter().rev() {
                                let callable =
                                    self.class_decorator_expression(decorator, namespace)?;
                                let value = self.value();
                                self.emit(
                                    statement.span,
                                    mir::OperationKind::Call {
                                        dest: value,
                                        callable,
                                        positional: vec![decorated],
                                        keywords: Vec::new(),
                                    },
                                );
                                decorated = value;
                            }
                            self.emit(
                                statement.span,
                                mir::OperationKind::ClassNamespaceSet {
                                    namespace,
                                    name: method_name.clone(),
                                    value: decorated,
                                },
                            );
                        }
                        hir::ClassMember::If {
                            condition,
                            then_body,
                            else_body,
                        } => self.lower_class_conditional_assignments(
                            statement.span,
                            namespace,
                            condition,
                            then_body,
                            else_body,
                        )?,
                        hir::ClassMember::While { condition, body } => self
                            .lower_class_while_assignments(
                                statement.span,
                                namespace,
                                condition,
                                body,
                            )?,
                        hir::ClassMember::For {
                            target,
                            iterable,
                            body,
                            else_body,
                        } => self.lower_class_for_members(
                            statement.span,
                            namespace,
                            target,
                            iterable,
                            body,
                            else_body,
                        )?,
                        hir::ClassMember::Expression(expression) => {
                            self.expression(expression)?;
                        }
                        hir::ClassMember::Print(values) => {
                            let values = values
                                .iter()
                                .map(|value| self.expression(value))
                                .collect::<Result<Vec<_>, _>>()?;
                            self.emit(statement.span, mir::OperationKind::Print { values });
                        }
                    }
                }
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
                self.class_scopes.pop();
                self.store_name(statement.span, name, *binding, decorated)?;
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
            hir::TargetKind::Name { name, .. } => {
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
            hir::TargetKind::Sequence { .. } | hir::TargetKind::Starred(_) => {
                return Err(
                    "sequence deletion target reached MIR before Gate 4 Slice 11".to_owned(),
                );
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
                    "sequence augmented target reached MIR before Gate 4 Slice 11".to_owned(),
                );
            }
        }
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
        }
        Ok(())
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
                hir::ClassMember::Assign { name, value } => {
                    let value = self.expression(value)?;
                    self.emit(
                        span,
                        mir::OperationKind::ClassNamespaceSet {
                            namespace,
                            name: name.clone(),
                            value,
                        },
                    );
                    self.record_class_name(name);
                }
                hir::ClassMember::AugAssign { name, op, value } => {
                    self.lower_class_augmented_assignment(span, namespace, name, *op, value)?;
                }
                hir::ClassMember::Delete { name } => self.emit(
                    span,
                    mir::OperationKind::ClassNamespaceDelete {
                        namespace,
                        name: name.clone(),
                    },
                ),
                hir::ClassMember::ItemAssign {
                    collection,
                    index,
                    value,
                } => {
                    let collection = self.expression(collection)?;
                    let index = self.expression(index)?;
                    let value = self.expression(value)?;
                    self.emit(
                        span,
                        mir::OperationKind::ItemSet {
                            collection,
                            index,
                            value,
                        },
                    );
                }
                hir::ClassMember::AttributeAssign {
                    receiver,
                    name,
                    value,
                } => {
                    let receiver = self.expression(receiver)?;
                    let value = self.expression(value)?;
                    self.emit(
                        span,
                        mir::OperationKind::AttributeSet {
                            receiver,
                            name: name.clone(),
                            value,
                        },
                    );
                }
                hir::ClassMember::AttributeDelete { receiver, name } => {
                    let receiver = self.expression(receiver)?;
                    self.emit(
                        span,
                        mir::OperationKind::AttributeDelete {
                            receiver,
                            name: name.clone(),
                        },
                    );
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
                hir::ClassMember::ClassDef {
                    name,
                    decorators,
                    bases,
                    metaclass,
                    keywords,
                    body,
                } => self.lower_nested_class_member(
                    span, namespace, name, decorators, bases, metaclass, keywords, body,
                )?,
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
                    name,
                    decorators,
                    uses_zero_argument_super,
                    parameters,
                    body,
                    locals,
                    cells: _,
                    free,
                } => {
                    let mut method_free = free.clone();
                    if *uses_zero_argument_super
                        && !method_free.iter().any(|name| name == "__class__")
                    {
                        method_free.push("__class__".to_owned());
                    }
                    let function = self.program.lower_scope(
                        name.clone(),
                        format!("<class>.{name}"),
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
                        span,
                        mir::OperationKind::MakeFunction {
                            dest: method,
                            function,
                            defaults,
                            closure,
                        },
                    );
                    let mut decorated = method;
                    for decorator in decorators.iter().rev() {
                        let callable = self.class_decorator_expression(decorator, namespace)?;
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
                    self.emit(
                        span,
                        mir::OperationKind::ClassNamespaceSet {
                            namespace,
                            name: name.clone(),
                            value: decorated,
                        },
                    );
                    self.record_class_name(name);
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
                binding: hir::Binding::Global,
                decorators: decorators.to_vec(),
                bases: bases.to_vec(),
                metaclass: metaclass.clone(),
                keywords: keywords.to_vec(),
                body: body.to_vec(),
            },
        };
        self.statement(&statement)?;
        let value = self.value();
        self.emit(
            span,
            mir::OperationKind::GlobalGet {
                dest: value,
                name: name.to_owned(),
            },
        );
        self.emit(
            span,
            mir::OperationKind::ClassNamespaceSet {
                namespace,
                name: name.to_owned(),
                value,
            },
        );
        self.emit(
            span,
            mir::OperationKind::GlobalDelete {
                name: name.to_owned(),
            },
        );
        self.record_class_name(name);
        Ok(())
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
                if let Some(name) = &handler.name {
                    self.emit(
                        span,
                        mir::OperationKind::ClassNamespaceSet {
                            namespace,
                            name: name.clone(),
                            value: handled,
                        },
                    );
                    self.record_class_name(name);
                }
                let cleanup_error = self.new_block();
                self.exception_target = Some(cleanup_error);
                self.cleanups.push(CleanupAction::ClassHandler {
                    namespace,
                    binding: handler.name.clone(),
                    exception_target: inner_exception,
                });
                self.lower_class_assignment_members(span, namespace, &handler.body)?;
                self.cleanups.pop();
                if self.is_open() {
                    if let Some(name) = &handler.name {
                        self.emit(
                            span,
                            mir::OperationKind::ClassNamespaceDelete {
                                namespace,
                                name: name.clone(),
                            },
                        );
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
                if let Some(name) = &handler.name {
                    self.emit(
                        span,
                        mir::OperationKind::ClassNamespaceDelete {
                            namespace,
                            name: name.clone(),
                        },
                    );
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

    fn lower_class_augmented_assignment(
        &mut self,
        span: Span,
        namespace: mir::ValueId,
        name: &str,
        op: hir::BinaryOperator,
        value: &hir::Expression,
    ) -> Result<(), String> {
        let current = self.value();
        self.emit(
            span,
            mir::OperationKind::ClassNamespaceGet {
                dest: current,
                namespace,
                name: name.to_owned(),
            },
        );
        let value = self.expression(value)?;
        let result = self.value();
        self.emit(
            span,
            mir::OperationKind::Binary {
                dest: result,
                op,
                left: current,
                right: value,
            },
        );
        self.emit(
            span,
            mir::OperationKind::ClassNamespaceSet {
                namespace,
                name: name.to_owned(),
                value: result,
            },
        );
        self.record_class_name(name);
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
                    namespace,
                    binding,
                    exception_target,
                } => {
                    self.exception_target = *exception_target;
                    if let Some(name) = binding {
                        self.emit(
                            Span::default(),
                            mir::OperationKind::ClassNamespaceDelete {
                                namespace: *namespace,
                                name: name.clone(),
                            },
                        );
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
        }
        Ok(())
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
                cells: _,
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
                    "<lambda>".to_owned(),
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
                }
            }
            hir::ExpressionKind::Name { name, binding } => match binding {
                hir::Binding::Global => {
                    let class_namespace = self.class_scopes.last().map(|scope| scope.namespace);
                    if let Some(namespace) = class_namespace {
                        mir::OperationKind::ClassNameGet {
                            dest: self.value(),
                            namespace,
                            name: name.clone(),
                        }
                    } else {
                        mir::OperationKind::GlobalGet {
                            dest: self.value(),
                            name: name.clone(),
                        }
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
            hir::ExpressionKind::Compare { op, left, right } => {
                let left = self.expression(left)?;
                let right = self.expression(right)?;
                if matches!(op, hir::CompareOperator::In | hir::CompareOperator::NotIn) {
                    mir::OperationKind::Contains {
                        dest: self.value(),
                        collection: right,
                        needle: left,
                        negate: *op == hir::CompareOperator::NotIn,
                    }
                } else {
                    mir::OperationKind::Compare {
                        dest: self.value(),
                        op: match op {
                            hir::CompareOperator::Equal => mir::CompareOperator::Equal,
                            hir::CompareOperator::NotEqual => mir::CompareOperator::NotEqual,
                            hir::CompareOperator::Less => mir::CompareOperator::Less,
                            hir::CompareOperator::LessEqual => mir::CompareOperator::LessEqual,
                            hir::CompareOperator::Greater => mir::CompareOperator::Greater,
                            hir::CompareOperator::GreaterEqual => {
                                mir::CompareOperator::GreaterEqual
                            }
                            hir::CompareOperator::In | hir::CompareOperator::NotIn => unreachable!(
                                "membership comparisons are lowered before comparison selection"
                            ),
                            hir::CompareOperator::Is => mir::CompareOperator::Is,
                            hir::CompareOperator::IsNot => mir::CompareOperator::IsNot,
                        },
                        left,
                        right,
                    }
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
                            hir::CallPart::Keyword { name, value } => (
                                mir::CallArgumentKind::Keyword,
                                Some(name.clone()),
                                value,
                            ),
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
}
