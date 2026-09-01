use std::collections::BTreeSet;
use std::path::Path;

use rimera_abi::RParameterKind;

use crate::core::{Diagnostic, DiagnosticSet, Span};
use crate::{hir, syntax};

pub fn analyze(path: &Path, module: &syntax::Module) -> Result<hir::Module, DiagnosticSet> {
    let raw = RawScope::module(&module.statements);
    let builtin_print_stable = !raw.mutates_global_name("print", true);
    let plan = resolve_scope(path, raw, &[])?;
    let mut analyzer = Analyzer {
        path,
        plan: &plan,
        child_index: 0,
        in_function: false,
        loop_depth: 0,
        in_except_star: false,
        handler_depth: 0,
        builtin_print_stable,
    };
    Ok(hir::Module {
        filename: module.filename.clone(),
        line_starts: module.line_starts.clone(),
        statements: analyzer.statements(&module.statements)?,
    })
}

#[derive(Debug)]
struct RawScope {
    is_function: bool,
    parameters: BTreeSet<String>,
    assigned: BTreeSet<String>,
    used: BTreeSet<String>,
    explicit_globals: BTreeSet<String>,
    nonlocals: BTreeSet<String>,
    children: Vec<RawScope>,
}

impl RawScope {
    fn module(statements: &[syntax::Statement]) -> Self {
        let mut scope = Self::empty(false);
        scope.scan_statements(statements);
        scope
    }

    fn function(parameters: &[syntax::Parameter], statements: &[syntax::Statement]) -> Self {
        let mut scope = Self::empty(true);
        scope
            .parameters
            .extend(parameters.iter().map(|parameter| parameter.name.clone()));
        scope.scan_statements(statements);
        scope
    }

    fn empty(is_function: bool) -> Self {
        Self {
            is_function,
            parameters: BTreeSet::new(),
            assigned: BTreeSet::new(),
            used: BTreeSet::new(),
            explicit_globals: BTreeSet::new(),
            nonlocals: BTreeSet::new(),
            children: Vec::new(),
        }
    }

    fn mutates_global_name(&self, name: &str, is_module: bool) -> bool {
        ((is_module || self.explicit_globals.contains(name)) && self.assigned.contains(name))
            || self
                .children
                .iter()
                .any(|child| child.mutates_global_name(name, false))
    }

    fn scan_statements(&mut self, statements: &[syntax::Statement]) {
        for statement in statements {
            match &statement.kind {
                syntax::StatementKind::Assign { targets, value } => {
                    for target in targets {
                        self.scan_target(target);
                    }
                    self.scan_expression(value);
                }
                syntax::StatementKind::AugAssign { target, value, .. } => {
                    self.scan_augmented_target(target);
                    self.scan_expression(value);
                }
                syntax::StatementKind::Delete { targets } => {
                    for target in targets {
                        self.scan_target(target);
                    }
                }
                syntax::StatementKind::FunctionDef {
                    name,
                    decorators,
                    parameters,
                    body,
                } => {
                    self.assigned.insert(name.clone());
                    for decorator in decorators {
                        self.scan_expression(decorator);
                    }
                    for parameter in parameters {
                        if let Some(default) = &parameter.default {
                            self.scan_expression(default);
                        }
                    }
                    self.children.push(Self::function(parameters, body));
                }
                syntax::StatementKind::ClassDef {
                    name,
                    decorators,
                    bases,
                    metaclass,
                    keywords,
                    body,
                } => {
                    self.assigned.insert(name.clone());
                    for decorator in decorators {
                        self.scan_expression(decorator);
                    }
                    for base in bases {
                        self.scan_expression(base);
                    }
                    if let Some(metaclass) = metaclass {
                        self.scan_expression(metaclass);
                    }
                    for (_, value) in keywords {
                        self.scan_expression(value);
                    }
                    self.scan_class_body(body);
                }
                syntax::StatementKind::Return { value } => {
                    if let Some(value) = value {
                        self.scan_expression(value);
                    }
                }
                syntax::StatementKind::Break | syntax::StatementKind::Continue => {}
                syntax::StatementKind::Expression(value) => self.scan_expression(value),
                syntax::StatementKind::Global(names) => {
                    self.explicit_globals.extend(names.iter().cloned());
                }
                syntax::StatementKind::Nonlocal(names) => {
                    self.nonlocals.extend(names.iter().cloned());
                }
                syntax::StatementKind::Raise { exception, cause } => {
                    if let Some(exception) = exception {
                        self.scan_expression(exception);
                    }
                    if let Some(cause) = cause {
                        self.scan_expression(cause);
                    }
                }
                syntax::StatementKind::Try {
                    body,
                    handlers,
                    else_body,
                    finally_body,
                    ..
                } => {
                    self.scan_statements(body);
                    for handler in handlers {
                        if let Some(exception_type) = &handler.exception_type {
                            self.scan_expression(exception_type);
                        }
                        if let Some(name) = &handler.name {
                            self.assigned.insert(name.clone());
                        }
                        self.scan_statements(&handler.body);
                    }
                    self.scan_statements(else_body);
                    self.scan_statements(finally_body);
                }
                syntax::StatementKind::Print { values } => {
                    values.iter().for_each(|value| self.scan_expression(value));
                }
                syntax::StatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.scan_expression(condition);
                    self.scan_statements(then_body);
                    self.scan_statements(else_body);
                }
                syntax::StatementKind::While { condition, body } => {
                    self.scan_expression(condition);
                    self.scan_statements(body);
                }
                syntax::StatementKind::For {
                    target,
                    iterable,
                    body,
                    else_body,
                } => {
                    self.scan_target(target);
                    self.scan_expression(iterable);
                    self.scan_statements(body);
                    self.scan_statements(else_body);
                }
            }
        }
    }

    fn scan_target(&mut self, target: &syntax::Target) {
        match &target.kind {
            syntax::TargetKind::Name(name) => {
                self.assigned.insert(name.clone());
            }
            syntax::TargetKind::Attribute { receiver, .. } => self.scan_expression(receiver),
            syntax::TargetKind::Item { collection, index } => {
                self.scan_expression(collection);
                self.scan_expression(index);
            }
            syntax::TargetKind::Sequence { elements, .. } => {
                for element in elements {
                    self.scan_target(element);
                }
            }
            syntax::TargetKind::Starred(target) => self.scan_target(target),
        }
    }

    fn scan_target_reads(&mut self, target: &syntax::Target) {
        match &target.kind {
            syntax::TargetKind::Name(_) => {}
            syntax::TargetKind::Attribute { receiver, .. } => self.scan_expression(receiver),
            syntax::TargetKind::Item { collection, index } => {
                self.scan_expression(collection);
                self.scan_expression(index);
            }
            syntax::TargetKind::Sequence { elements, .. } => {
                for element in elements {
                    self.scan_target_reads(element);
                }
            }
            syntax::TargetKind::Starred(target) => self.scan_target_reads(target),
        }
    }

    fn scan_augmented_target(&mut self, target: &syntax::Target) {
        if let syntax::TargetKind::Name(name) = &target.kind {
            self.assigned.insert(name.clone());
            self.used.insert(name.clone());
        } else {
            self.scan_target_reads(target);
        }
    }

    fn scan_class_body(&mut self, statements: &[syntax::Statement]) {
        for statement in statements {
            match &statement.kind {
                syntax::StatementKind::Assign { targets, value } => {
                    for target in targets {
                        self.scan_target_reads(target);
                    }
                    self.scan_expression(value);
                }
                syntax::StatementKind::AugAssign { target, value, .. } => {
                    self.scan_target_reads(target);
                    self.scan_expression(value);
                }
                syntax::StatementKind::Delete { targets } => {
                    for target in targets {
                        self.scan_target_reads(target);
                    }
                }
                syntax::StatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.scan_expression(condition);
                    self.scan_class_body(then_body);
                    self.scan_class_body(else_body);
                }
                syntax::StatementKind::While { condition, body } => {
                    self.scan_expression(condition);
                    self.scan_class_body(body);
                }
                syntax::StatementKind::For {
                    target,
                    iterable,
                    body,
                    else_body,
                } => {
                    self.scan_target_reads(target);
                    self.scan_expression(iterable);
                    self.scan_class_body(body);
                    self.scan_class_body(else_body);
                }
                syntax::StatementKind::Expression(expression) => self.scan_expression(expression),
                syntax::StatementKind::Print { values } => {
                    values.iter().for_each(|value| self.scan_expression(value));
                }
                syntax::StatementKind::FunctionDef {
                    decorators,
                    parameters,
                    body,
                    ..
                } => {
                    for decorator in decorators {
                        self.scan_expression(decorator);
                    }
                    for parameter in parameters {
                        if let Some(default) = &parameter.default {
                            self.scan_expression(default);
                        }
                    }
                    self.children.push(Self::function(parameters, body));
                }
                _ => {}
            }
        }
    }

    fn scan_expression(&mut self, expression: &syntax::Expression) {
        match &expression.kind {
            syntax::ExpressionKind::Name(name) => {
                self.used.insert(name.clone());
            }
            syntax::ExpressionKind::List(values)
            | syntax::ExpressionKind::Tuple(values)
            | syntax::ExpressionKind::Set(values) => {
                values.iter().for_each(|value| self.scan_expression(value));
            }
            syntax::ExpressionKind::Dictionary(entries) => {
                entries.iter().for_each(|entry| match entry {
                    syntax::DictionaryEntry::Pair { key, value } => {
                        self.scan_expression(key);
                        self.scan_expression(value);
                    }
                    syntax::DictionaryEntry::Unpack(value) => self.scan_expression(value),
                })
            }
            syntax::ExpressionKind::Subscript { value, index } => {
                self.scan_expression(value);
                self.scan_expression(index);
            }
            syntax::ExpressionKind::Slice { start, stop, step } => {
                for value in [start, stop, step].into_iter().flatten() {
                    self.scan_expression(value);
                }
            }
            syntax::ExpressionKind::Attribute { value, .. } => self.scan_expression(value),
            syntax::ExpressionKind::Lambda { parameters, body } => {
                for parameter in parameters {
                    if let Some(default) = &parameter.default {
                        self.scan_expression(default);
                    }
                }
                let mut child = Self::empty(true);
                child
                    .parameters
                    .extend(parameters.iter().map(|parameter| parameter.name.clone()));
                child.scan_expression(body);
                self.children.push(child);
            }
            syntax::ExpressionKind::Unary { operand, .. } => self.scan_expression(operand),
            syntax::ExpressionKind::Boolean { values, .. } => {
                values.iter().for_each(|value| self.scan_expression(value));
            }
            syntax::ExpressionKind::Binary { left, right, .. }
            | syntax::ExpressionKind::Compare { left, right, .. } => {
                self.scan_expression(left);
                self.scan_expression(right);
            }
            syntax::ExpressionKind::Call { callable, parts } => {
                self.scan_expression(callable);
                parts.iter().for_each(|part| match part {
                    syntax::CallPart::Positional(value)
                    | syntax::CallPart::Starred(value)
                    | syntax::CallPart::Keyword { value, .. }
                    | syntax::CallPart::KeywordUnpack(value) => self.scan_expression(value),
                });
            }
            syntax::ExpressionKind::None
            | syntax::ExpressionKind::Bool(_)
            | syntax::ExpressionKind::Int(_)
            | syntax::ExpressionKind::Float(_)
            | syntax::ExpressionKind::String(_)
            | syntax::ExpressionKind::Bytes(_)
            | syntax::ExpressionKind::Complex { .. } => {}
        }
    }
}

#[derive(Debug)]
struct ScopePlan {
    is_function: bool,
    locals: BTreeSet<String>,
    cells: BTreeSet<String>,
    free: BTreeSet<String>,
    explicit_globals: BTreeSet<String>,
    children: Vec<ScopePlan>,
}

fn resolve_scope(
    path: &Path,
    raw: RawScope,
    ancestors: &[BTreeSet<String>],
) -> Result<ScopePlan, DiagnosticSet> {
    if !raw.explicit_globals.is_disjoint(&raw.nonlocals) {
        return Err(sema_error(
            path,
            "a name cannot be declared both global and nonlocal",
        ));
    }
    if !raw.parameters.is_disjoint(&raw.explicit_globals)
        || !raw.parameters.is_disjoint(&raw.nonlocals)
    {
        return Err(sema_error(
            path,
            "a parameter cannot be declared global or nonlocal",
        ));
    }
    if !raw.is_function && !raw.nonlocals.is_empty() {
        return Err(sema_error(
            path,
            "nonlocal declaration is not allowed at module scope",
        ));
    }

    let mut locals = raw.parameters.clone();
    locals.extend(raw.assigned.iter().cloned());
    locals.retain(|name| !raw.explicit_globals.contains(name) && !raw.nonlocals.contains(name));
    for name in &raw.nonlocals {
        if !ancestors.iter().rev().any(|scope| scope.contains(name)) {
            return Err(sema_error(
                path,
                format!("no binding for nonlocal `{name}` was found"),
            ));
        }
    }

    let mut free = BTreeSet::new();
    if raw.is_function {
        for name in raw.used.iter().chain(raw.nonlocals.iter()) {
            if !locals.contains(name)
                && !raw.explicit_globals.contains(name)
                && ancestors.iter().rev().any(|scope| scope.contains(name))
            {
                free.insert(name.clone());
            }
        }
    }
    let mut child_ancestors = ancestors.to_vec();
    if raw.is_function {
        child_ancestors.push(locals.clone());
    }
    let mut children = Vec::with_capacity(raw.children.len());
    let mut cells = BTreeSet::new();
    for child in raw.children {
        let child = resolve_scope(path, child, &child_ancestors)?;
        for name in &child.free {
            if locals.contains(name) {
                cells.insert(name.clone());
            } else if raw.is_function && ancestors.iter().rev().any(|scope| scope.contains(name)) {
                free.insert(name.clone());
            }
        }
        children.push(child);
    }
    Ok(ScopePlan {
        is_function: raw.is_function,
        locals,
        cells,
        free,
        explicit_globals: raw.explicit_globals,
        children,
    })
}

struct Analyzer<'a> {
    path: &'a Path,
    plan: &'a ScopePlan,
    child_index: usize,
    in_function: bool,
    loop_depth: usize,
    in_except_star: bool,
    handler_depth: usize,
    builtin_print_stable: bool,
}

impl Analyzer<'_> {
    fn statements(
        &mut self,
        statements: &[syntax::Statement],
    ) -> Result<Vec<hir::Statement>, DiagnosticSet> {
        let mut result = Vec::new();
        for statement in statements {
            if let Some(statement) = self.statement(statement)? {
                result.push(statement);
            }
        }
        Ok(result)
    }

    fn statement(
        &mut self,
        statement: &syntax::Statement,
    ) -> Result<Option<hir::Statement>, DiagnosticSet> {
        let kind = match &statement.kind {
            syntax::StatementKind::Assign { targets, value } => {
                let targets = targets
                    .iter()
                    .map(|target| self.target(target))
                    .collect::<Result<Vec<_>, _>>()?;
                for target in &targets {
                    self.validate_assignment_target(target)?;
                }
                hir::StatementKind::Assign {
                    targets,
                    value: self.expression(value)?,
                }
            }
            syntax::StatementKind::AugAssign { target, op, value } => {
                let target = self.target(target)?;
                self.validate_augmented_target(&target)?;
                hir::StatementKind::AugAssign {
                    target,
                    op: *op,
                    value: self.expression(value)?,
                }
            }
            syntax::StatementKind::Delete { targets } => {
                let targets = targets
                    .iter()
                    .map(|target| self.target(target))
                    .collect::<Result<Vec<_>, _>>()?;
                for target in &targets {
                    self.validate_delete_target(target)?;
                }
                hir::StatementKind::Delete { targets }
            }
            syntax::StatementKind::FunctionDef {
                name,
                decorators,
                parameters,
                body,
            } => {
                if !decorators.is_empty() {
                    return Err(sema_error(
                        self.path,
                        "function decorators are supported only in class bodies",
                    ));
                }
                let defaults = parameters
                    .iter()
                    .map(|parameter| {
                        parameter
                            .default
                            .as_ref()
                            .map(|default| self.expression(default))
                            .transpose()
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let child = self
                    .plan
                    .children
                    .get(self.child_index)
                    .ok_or_else(|| sema_error(self.path, "function scope plan is missing"))?;
                self.child_index += 1;
                let mut analyzer = Analyzer {
                    path: self.path,
                    plan: child,
                    child_index: 0,
                    in_function: true,
                    loop_depth: 0,
                    in_except_star: false,
                    handler_depth: 0,
                    builtin_print_stable: self.builtin_print_stable,
                };
                let parameters = parameters
                    .iter()
                    .zip(defaults)
                    .map(|(parameter, default)| hir::Parameter {
                        name: parameter.name.clone(),
                        kind: match parameter.kind {
                            syntax::ParameterKind::PositionalOnly => RParameterKind::PositionalOnly,
                            syntax::ParameterKind::PositionalOrKeyword => {
                                RParameterKind::PositionalOrKeyword
                            }
                            syntax::ParameterKind::VarArgs => RParameterKind::VarArgs,
                            syntax::ParameterKind::KeywordOnly => RParameterKind::KeywordOnly,
                            syntax::ParameterKind::VarKeywords => RParameterKind::VarKeywords,
                        },
                        default,
                    })
                    .collect();
                hir::StatementKind::FunctionDef {
                    name: name.clone(),
                    binding: self.binding(name),
                    decorators: Vec::new(),
                    parameters,
                    body: analyzer.statements(body)?,
                    locals: child.locals.iter().cloned().collect(),
                    cells: child.cells.iter().cloned().collect(),
                    free: child.free.iter().cloned().collect(),
                }
            }
            syntax::StatementKind::ClassDef {
                name,
                decorators,
                bases,
                metaclass,
                keywords,
                body,
            } => {
                if self.in_function {
                    return Err(sema_error(
                        self.path,
                        "class definitions are supported only at module scope",
                    ));
                }
                hir::StatementKind::ClassDef {
                    name: name.clone(),
                    binding: self.binding(name),
                    decorators: decorators
                        .iter()
                        .map(|decorator| self.expression(decorator))
                        .collect::<Result<Vec<_>, _>>()?,
                    bases: bases
                        .iter()
                        .map(|base| self.expression(base))
                        .collect::<Result<Vec<_>, _>>()?,
                    metaclass: metaclass
                        .as_ref()
                        .map(|value| self.expression(value))
                        .transpose()?,
                    keywords: keywords
                        .iter()
                        .map(|(name, value)| Ok((name.clone(), self.expression(value)?)))
                        .collect::<Result<_, DiagnosticSet>>()?,
                    body: self.class_members(body)?,
                }
            }
            syntax::StatementKind::Return { value } => {
                if !self.in_function {
                    return Err(sema_error(
                        self.path,
                        "return is not allowed at module scope",
                    ));
                }
                if self.in_except_star {
                    return Err(sema_error(
                        self.path,
                        "`return` is not allowed in an `except*` handler",
                    ));
                }
                hir::StatementKind::Return {
                    value: value
                        .as_ref()
                        .map(|value| self.expression(value))
                        .transpose()?,
                }
            }
            syntax::StatementKind::Break => {
                if self.in_except_star {
                    return Err(sema_error(
                        self.path,
                        "`break` is not allowed in an `except*` handler",
                    ));
                }
                if self.loop_depth == 0 {
                    return Err(sema_error(self.path, "`break` is only valid inside a loop"));
                }
                hir::StatementKind::Break
            }
            syntax::StatementKind::Continue => {
                if self.in_except_star {
                    return Err(sema_error(
                        self.path,
                        "`continue` is not allowed in an `except*` handler",
                    ));
                }
                if self.loop_depth == 0 {
                    return Err(sema_error(
                        self.path,
                        "`continue` is only valid inside a loop",
                    ));
                }
                hir::StatementKind::Continue
            }
            syntax::StatementKind::Expression(expression) => {
                if let syntax::ExpressionKind::Call {
                    callable,
                    positional,
                    keywords,
                } = &expression.kind
                    && keywords.is_empty()
                    && self.builtin_print_stable
                    && self.binding("print") == hir::Binding::Global
                    && matches!(
                        &callable.kind,
                        syntax::ExpressionKind::Name(name) if name == "print"
                    )
                {
                    hir::StatementKind::Print {
                        values: positional
                            .iter()
                            .map(|value| self.expression(value))
                            .collect::<Result<_, _>>()?,
                    }
                } else {
                    hir::StatementKind::Expression(self.expression(expression)?)
                }
            }
            syntax::StatementKind::Raise { exception, cause } => {
                if exception.is_none() && self.handler_depth == 0 {
                    return Err(sema_error(
                        self.path,
                        "bare `raise` requires an active exception handler",
                    ));
                }
                hir::StatementKind::Raise {
                    exception: exception
                        .as_ref()
                        .map(|exception| self.expression(exception))
                        .transpose()?,
                    cause: cause
                        .as_ref()
                        .map(|cause| self.expression(cause))
                        .transpose()?,
                }
            }
            syntax::StatementKind::Try {
                body,
                handlers,
                else_body,
                finally_body,
                is_star,
            } => {
                let body = self.statements(body)?;
                let mut resolved_handlers = Vec::with_capacity(handlers.len());
                for handler in handlers {
                    let exception_type = handler
                        .exception_type
                        .as_ref()
                        .map(|exception_type| self.expression(exception_type))
                        .transpose()?;
                    let name = handler
                        .name
                        .as_ref()
                        .map(|name| (name.clone(), self.binding(name)));
                    let previous = self.in_except_star;
                    self.in_except_star = *is_star;
                    self.handler_depth += 1;
                    let handler_body = self.statements(&handler.body);
                    self.handler_depth -= 1;
                    self.in_except_star = previous;
                    resolved_handlers.push(hir::ExceptionHandler {
                        exception_type,
                        name,
                        body: handler_body?,
                    });
                }
                hir::StatementKind::Try {
                    body,
                    handlers: resolved_handlers,
                    else_body: self.statements(else_body)?,
                    finally_body: self.statements(finally_body)?,
                    is_star: *is_star,
                }
            }
            syntax::StatementKind::Global(_) | syntax::StatementKind::Nonlocal(_) => {
                return Ok(None);
            }
            syntax::StatementKind::Print { values } => hir::StatementKind::Print {
                values: values
                    .iter()
                    .map(|value| self.expression(value))
                    .collect::<Result<_, _>>()?,
            },
            syntax::StatementKind::If {
                condition,
                then_body,
                else_body,
            } => hir::StatementKind::If {
                condition: self.expression(condition)?,
                then_body: self.statements(then_body)?,
                else_body: self.statements(else_body)?,
            },
            syntax::StatementKind::While { condition, body } => {
                let condition = self.expression(condition)?;
                self.loop_depth += 1;
                let body = self.statements(body);
                self.loop_depth -= 1;
                hir::StatementKind::While {
                    condition,
                    body: body?,
                }
            }
            syntax::StatementKind::For {
                target,
                iterable,
                body,
                else_body,
            } => {
                let target = self.target(target)?;
                self.validate_loop_target(&target)?;
                let iterable = self.expression(iterable)?;
                self.loop_depth += 1;
                let body = self.statements(body);
                self.loop_depth -= 1;
                hir::StatementKind::For {
                    target,
                    iterable,
                    body: body?,
                    else_body: self.statements(else_body)?,
                }
            }
        };
        Ok(Some(hir::Statement {
            span: statement.span,
            kind,
        }))
    }

    fn class_members(
        &mut self,
        statements: &[syntax::Statement],
    ) -> Result<Vec<hir::ClassMember>, DiagnosticSet> {
        let mut members = Vec::with_capacity(statements.len());
        for statement in statements {
            let member = match &statement.kind {
                syntax::StatementKind::Assign { targets, value } => {
                    let [target] = targets.as_slice() else {
                        return Err(capability_error(
                            self.path,
                            statement.span,
                            "RIM-CAP-G4-03",
                            "chained class-body target writes are owned by Gate 4 target execution slices",
                        ));
                    };
                    match &target.kind {
                        syntax::TargetKind::Name(name) => hir::ClassMember::Assign {
                            name: name.clone(),
                            value: self.expression(value)?,
                        },
                        syntax::TargetKind::Item { collection, index } => {
                            hir::ClassMember::ItemAssign {
                                collection: self.expression(collection)?,
                                index: self.expression(index)?,
                                value: self.expression(value)?,
                            }
                        }
                        syntax::TargetKind::Attribute { receiver, name } => {
                            hir::ClassMember::AttributeAssign {
                                receiver: self.expression(receiver)?,
                                name: name.clone(),
                                value: self.expression(value)?,
                            }
                        }
                        syntax::TargetKind::Sequence { .. } => {
                            return Err(capability_error(
                                self.path,
                                target.span,
                                "RIM-CAP-G4-03",
                                "nested class-body unpacking is owned by Gate 4 Slice 3",
                            ));
                        }
                        syntax::TargetKind::Starred(_) => {
                            return Err(capability_error(
                                self.path,
                                target.span,
                                "RIM-CAP-G4-04",
                                "starred class-body unpacking is owned by Gate 4 Slice 4",
                            ));
                        }
                    }
                }
                syntax::StatementKind::AugAssign { target, op, value } => match &target.kind {
                    syntax::TargetKind::Name(name) => hir::ClassMember::AugAssign {
                        name: name.clone(),
                        op: *op,
                        value: self.expression(value)?,
                    },
                    _ => {
                        return Err(capability_error(
                            self.path,
                            target.span,
                            "RIM-CAP-G4-11",
                            "non-name augmented class-body targets are owned by Gate 4 Slice 11",
                        ));
                    }
                },
                syntax::StatementKind::Delete { targets } => {
                    let [target] = targets.as_slice() else {
                        return Err(capability_error(
                            self.path,
                            statement.span,
                            "RIM-CAP-G4-11",
                            "general deletion targets are owned by Gate 4 Slice 11",
                        ));
                    };
                    match &target.kind {
                        syntax::TargetKind::Name(name) => {
                            hir::ClassMember::Delete { name: name.clone() }
                        }
                        syntax::TargetKind::Attribute { receiver, name } => {
                            hir::ClassMember::AttributeDelete {
                                receiver: self.expression(receiver)?,
                                name: name.clone(),
                            }
                        }
                        _ => {
                            return Err(capability_error(
                                self.path,
                                target.span,
                                "RIM-CAP-G4-11",
                                "general deletion targets are owned by Gate 4 Slice 11",
                            ));
                        }
                    }
                }
                syntax::StatementKind::Raise { exception, cause } => hir::ClassMember::Raise {
                    exception: exception
                        .as_ref()
                        .map(|value| self.expression(value))
                        .transpose()?,
                    cause: cause
                        .as_ref()
                        .map(|value| self.expression(value))
                        .transpose()?,
                },
                syntax::StatementKind::Try {
                    body,
                    handlers,
                    else_body,
                    finally_body,
                    is_star,
                } => hir::ClassMember::Try {
                    body: self.class_members(body)?,
                    handlers: handlers
                        .iter()
                        .map(|handler| {
                            Ok(hir::ClassExceptionHandler {
                                exception_type: handler
                                    .exception_type
                                    .as_ref()
                                    .map(|value| self.expression(value))
                                    .transpose()?,
                                name: handler.name.clone(),
                                body: self.class_members(&handler.body)?,
                            })
                        })
                        .collect::<Result<_, DiagnosticSet>>()?,
                    else_body: self.class_members(else_body)?,
                    finally_body: self.class_members(finally_body)?,
                    is_star: *is_star,
                },
                syntax::StatementKind::ClassDef {
                    name,
                    decorators,
                    bases,
                    metaclass,
                    keywords,
                    body,
                } => hir::ClassMember::ClassDef {
                    name: name.clone(),
                    decorators: decorators
                        .iter()
                        .map(|decorator| self.expression(decorator))
                        .collect::<Result<_, _>>()?,
                    bases: bases
                        .iter()
                        .map(|base| self.expression(base))
                        .collect::<Result<_, _>>()?,
                    metaclass: metaclass
                        .as_ref()
                        .map(|value| self.expression(value))
                        .transpose()?,
                    keywords: keywords
                        .iter()
                        .map(|(name, value)| Ok((name.clone(), self.expression(value)?)))
                        .collect::<Result<_, DiagnosticSet>>()?,
                    body: self.class_members(body)?,
                },
                syntax::StatementKind::Break => {
                    if self.loop_depth == 0 {
                        return Err(sema_error(self.path, "`break` is only valid inside a loop"));
                    }
                    hir::ClassMember::Break
                }
                syntax::StatementKind::Continue => {
                    if self.loop_depth == 0 {
                        return Err(sema_error(
                            self.path,
                            "`continue` is only valid inside a loop",
                        ));
                    }
                    hir::ClassMember::Continue
                }
                syntax::StatementKind::FunctionDef {
                    name,
                    decorators,
                    parameters,
                    body,
                } => {
                    let defaults = parameters
                        .iter()
                        .map(|parameter| {
                            parameter
                                .default
                                .as_ref()
                                .map(|default| self.expression(default))
                                .transpose()
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let child =
                        self.plan.children.get(self.child_index).ok_or_else(|| {
                            sema_error(self.path, "function scope plan is missing")
                        })?;
                    self.child_index += 1;
                    let mut analyzer = Analyzer {
                        path: self.path,
                        plan: child,
                        child_index: 0,
                        in_function: true,
                        loop_depth: 0,
                        in_except_star: false,
                        handler_depth: 0,
                        builtin_print_stable: self.builtin_print_stable,
                    };
                    let parameters = parameters
                        .iter()
                        .zip(defaults)
                        .map(|(parameter, default)| hir::Parameter {
                            name: parameter.name.clone(),
                            kind: match parameter.kind {
                                syntax::ParameterKind::PositionalOnly => {
                                    RParameterKind::PositionalOnly
                                }
                                syntax::ParameterKind::PositionalOrKeyword => {
                                    RParameterKind::PositionalOrKeyword
                                }
                                syntax::ParameterKind::VarArgs => RParameterKind::VarArgs,
                                syntax::ParameterKind::KeywordOnly => RParameterKind::KeywordOnly,
                                syntax::ParameterKind::VarKeywords => RParameterKind::VarKeywords,
                            },
                            default,
                        })
                        .collect();
                    hir::ClassMember::FunctionDef {
                        name: name.clone(),
                        decorators: decorators
                            .iter()
                            .map(|decorator| self.expression(decorator))
                            .collect::<Result<Vec<_>, _>>()?,
                        uses_zero_argument_super: contains_zero_argument_super(body),
                        parameters,
                        body: analyzer.statements(body)?,
                        locals: child.locals.iter().cloned().collect(),
                        cells: child.cells.iter().cloned().collect(),
                        free: child.free.iter().cloned().collect(),
                    }
                }
                syntax::StatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => hir::ClassMember::If {
                    condition: self.expression(condition)?,
                    then_body: self.class_members(then_body)?,
                    else_body: self.class_members(else_body)?,
                },
                syntax::StatementKind::While { condition, body } => {
                    let condition = self.expression(condition)?;
                    self.loop_depth += 1;
                    let body = self.class_members(body);
                    self.loop_depth -= 1;
                    hir::ClassMember::While {
                        condition,
                        body: body?,
                    }
                }
                syntax::StatementKind::For {
                    target,
                    iterable,
                    body,
                    else_body,
                } => {
                    let target = self.target(target)?;
                    self.validate_loop_target(&target)?;
                    let iterable = self.expression(iterable)?;
                    self.loop_depth += 1;
                    let body = self.class_members(body);
                    self.loop_depth -= 1;
                    hir::ClassMember::For {
                        target,
                        iterable,
                        body: body?,
                        else_body: self.class_members(else_body)?,
                    }
                }
                syntax::StatementKind::Expression(expression) => {
                    hir::ClassMember::Expression(self.expression(expression)?)
                }
                syntax::StatementKind::Print { values } => hir::ClassMember::Print(
                    values
                        .iter()
                        .map(|value| self.expression(value))
                        .collect::<Result<_, _>>()?,
                ),
                _ => {
                    return Err(sema_error(
                        self.path,
                        "class body contains an invalid member",
                    ));
                }
            };
            members.push(member);
        }
        Ok(members)
    }

    fn target(&mut self, target: &syntax::Target) -> Result<hir::Target, DiagnosticSet> {
        let kind = match &target.kind {
            syntax::TargetKind::Name(name) => hir::TargetKind::Name {
                name: name.clone(),
                binding: self.binding(name),
            },
            syntax::TargetKind::Attribute { receiver, name } => hir::TargetKind::Attribute {
                receiver: self.expression(receiver)?,
                name: name.clone(),
            },
            syntax::TargetKind::Item { collection, index } => hir::TargetKind::Item {
                collection: self.expression(collection)?,
                index: self.expression(index)?,
            },
            syntax::TargetKind::Sequence { kind, elements } => hir::TargetKind::Sequence {
                kind: match kind {
                    syntax::SequenceTargetKind::Tuple => hir::SequenceTargetKind::Tuple,
                    syntax::SequenceTargetKind::List => hir::SequenceTargetKind::List,
                },
                elements: elements
                    .iter()
                    .map(|element| self.target(element))
                    .collect::<Result<Vec<_>, _>>()?,
            },
            syntax::TargetKind::Starred(target) => {
                hir::TargetKind::Starred(Box::new(self.target(target)?))
            }
        };
        let target = hir::Target {
            span: target.span,
            kind,
        };
        self.validate_target_tree(&target)?;
        Ok(target)
    }

    fn validate_target_tree(&self, target: &hir::Target) -> Result<(), DiagnosticSet> {
        match &target.kind {
            hir::TargetKind::Sequence { elements, .. } => {
                let stars = elements
                    .iter()
                    .filter(|element| matches!(element.kind, hir::TargetKind::Starred(_)))
                    .count();
                if stars > 1 {
                    return Err(capability_error(
                        self.path,
                        target.span,
                        "RIM-CAP-G4-02",
                        "a recursive assignment target may contain at most one star per sequence level",
                    ));
                }
                for element in elements {
                    self.validate_target_tree(element)?;
                }
            }
            hir::TargetKind::Starred(target) => self.validate_target_tree(target)?,
            hir::TargetKind::Name { .. }
            | hir::TargetKind::Attribute { .. }
            | hir::TargetKind::Item { .. } => {}
        }
        Ok(())
    }

    fn validate_assignment_target(&self, target: &hir::Target) -> Result<(), DiagnosticSet> {
        match &target.kind {
            hir::TargetKind::Name { .. }
            | hir::TargetKind::Attribute { .. }
            | hir::TargetKind::Item { .. } => Ok(()),
            hir::TargetKind::Sequence { elements, .. } => {
                for element in elements {
                    self.validate_assignment_target(element)?;
                }
                Ok(())
            }
            hir::TargetKind::Starred(inner) => self.validate_assignment_target(inner),
        }
    }

    fn validate_augmented_target(&self, target: &hir::Target) -> Result<(), DiagnosticSet> {
        match &target.kind {
            hir::TargetKind::Name { .. }
            | hir::TargetKind::Attribute { .. }
            | hir::TargetKind::Item { .. } => Ok(()),
            _ => Err(capability_error(
                self.path,
                target.span,
                "RIM-CAP-G4-11",
                "this augmented-assignment target shape is owned by Gate 4 Slice 11",
            )),
        }
    }

    fn validate_delete_target(&self, target: &hir::Target) -> Result<(), DiagnosticSet> {
        match &target.kind {
            hir::TargetKind::Name { .. }
            | hir::TargetKind::Attribute { .. }
            | hir::TargetKind::Item { .. } => Ok(()),
            _ => Err(capability_error(
                self.path,
                target.span,
                "RIM-CAP-G4-11",
                "general deletion targets are owned by Gate 4 Slice 11",
            )),
        }
    }

    fn validate_loop_target(&self, target: &hir::Target) -> Result<(), DiagnosticSet> {
        self.validate_assignment_target(target)
    }

    fn expression(
        &mut self,
        expression: &syntax::Expression,
    ) -> Result<hir::Expression, DiagnosticSet> {
        let kind = match &expression.kind {
            syntax::ExpressionKind::None => hir::ExpressionKind::None,
            syntax::ExpressionKind::Bool(value) => hir::ExpressionKind::Bool(*value),
            syntax::ExpressionKind::Int(value) => hir::ExpressionKind::Int(value.clone()),
            syntax::ExpressionKind::Float(value) => hir::ExpressionKind::Float(*value),
            syntax::ExpressionKind::String(value) => hir::ExpressionKind::String(value.clone()),
            syntax::ExpressionKind::Bytes(value) => hir::ExpressionKind::Bytes(value.clone()),
            syntax::ExpressionKind::Complex { real, imag } => hir::ExpressionKind::Complex {
                real: *real,
                imag: *imag,
            },
            syntax::ExpressionKind::Slice { start, stop, step } => hir::ExpressionKind::Slice {
                start: start
                    .as_deref()
                    .map(|value| self.expression(value))
                    .transpose()?
                    .map(Box::new),
                stop: stop
                    .as_deref()
                    .map(|value| self.expression(value))
                    .transpose()?
                    .map(Box::new),
                step: step
                    .as_deref()
                    .map(|value| self.expression(value))
                    .transpose()?
                    .map(Box::new),
            },
            syntax::ExpressionKind::List(values) => hir::ExpressionKind::List(
                values
                    .iter()
                    .map(|value| self.expression(value))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            syntax::ExpressionKind::Tuple(values) => hir::ExpressionKind::Tuple(
                values
                    .iter()
                    .map(|value| self.expression(value))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            syntax::ExpressionKind::Dictionary(entries) => hir::ExpressionKind::Dictionary(
                entries
                    .iter()
                    .map(|entry| match entry {
                        syntax::DictionaryEntry::Pair { key, value } => {
                            Ok(hir::DictionaryEntry::Pair {
                                key: self.expression(key)?,
                                value: self.expression(value)?,
                            })
                        }
                        syntax::DictionaryEntry::Unpack(value) => {
                            Ok(hir::DictionaryEntry::Unpack(self.expression(value)?))
                        }
                    })
                    .collect::<Result<Vec<_>, DiagnosticSet>>()?,
            ),
            syntax::ExpressionKind::Set(values) => hir::ExpressionKind::Set(
                values
                    .iter()
                    .map(|value| self.expression(value))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            syntax::ExpressionKind::Subscript { value, index } => hir::ExpressionKind::Subscript {
                value: Box::new(self.expression(value)?),
                index: Box::new(self.expression(index)?),
            },
            syntax::ExpressionKind::Attribute { value, name } => hir::ExpressionKind::Attribute {
                value: Box::new(self.expression(value)?),
                name: name.clone(),
            },
            syntax::ExpressionKind::Lambda { parameters, body } => {
                let defaults = parameters
                    .iter()
                    .map(|parameter| {
                        parameter
                            .default
                            .as_ref()
                            .map(|default| self.expression(default))
                            .transpose()
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let child = self
                    .plan
                    .children
                    .get(self.child_index)
                    .ok_or_else(|| sema_error(self.path, "lambda scope plan is missing"))?;
                self.child_index += 1;
                let mut analyzer = Analyzer {
                    path: self.path,
                    plan: child,
                    child_index: 0,
                    in_function: true,
                    loop_depth: 0,
                    in_except_star: false,
                    handler_depth: 0,
                    builtin_print_stable: self.builtin_print_stable,
                };
                let parameters = parameters
                    .iter()
                    .zip(defaults)
                    .map(|(parameter, default)| hir::Parameter {
                        name: parameter.name.clone(),
                        kind: match parameter.kind {
                            syntax::ParameterKind::PositionalOnly => RParameterKind::PositionalOnly,
                            syntax::ParameterKind::PositionalOrKeyword => {
                                RParameterKind::PositionalOrKeyword
                            }
                            syntax::ParameterKind::VarArgs => RParameterKind::VarArgs,
                            syntax::ParameterKind::KeywordOnly => RParameterKind::KeywordOnly,
                            syntax::ParameterKind::VarKeywords => RParameterKind::VarKeywords,
                        },
                        default,
                    })
                    .collect();
                hir::ExpressionKind::Lambda {
                    parameters,
                    body: Box::new(analyzer.expression(body)?),
                    locals: child.locals.iter().cloned().collect(),
                    cells: child.cells.iter().cloned().collect(),
                    free: child.free.iter().cloned().collect(),
                }
            }
            syntax::ExpressionKind::Name(name) => hir::ExpressionKind::Name {
                name: name.clone(),
                binding: self.binding(name),
            },
            syntax::ExpressionKind::Unary { op, operand } => hir::ExpressionKind::Unary {
                op: match op {
                    syntax::UnaryOperator::Positive => hir::UnaryOperator::Positive,
                    syntax::UnaryOperator::Negate => hir::UnaryOperator::Negate,
                    syntax::UnaryOperator::Invert => hir::UnaryOperator::Invert,
                    syntax::UnaryOperator::Not => hir::UnaryOperator::Not,
                },
                operand: Box::new(self.expression(operand)?),
            },
            syntax::ExpressionKind::Boolean { op, values } => hir::ExpressionKind::Boolean {
                op: match op {
                    syntax::BooleanOperator::And => hir::BooleanOperator::And,
                    syntax::BooleanOperator::Or => hir::BooleanOperator::Or,
                },
                values: values
                    .iter()
                    .map(|value| self.expression(value))
                    .collect::<Result<Vec<_>, _>>()?,
            },
            syntax::ExpressionKind::Binary { op, left, right } => hir::ExpressionKind::Binary {
                op: *op,
                left: Box::new(self.expression(left)?),
                right: Box::new(self.expression(right)?),
            },
            syntax::ExpressionKind::Compare { op, left, right } => hir::ExpressionKind::Compare {
                op: match op {
                    syntax::CompareOperator::Equal => hir::CompareOperator::Equal,
                    syntax::CompareOperator::NotEqual => hir::CompareOperator::NotEqual,
                    syntax::CompareOperator::Less => hir::CompareOperator::Less,
                    syntax::CompareOperator::LessEqual => hir::CompareOperator::LessEqual,
                    syntax::CompareOperator::Greater => hir::CompareOperator::Greater,
                    syntax::CompareOperator::GreaterEqual => hir::CompareOperator::GreaterEqual,
                    syntax::CompareOperator::In => hir::CompareOperator::In,
                    syntax::CompareOperator::NotIn => hir::CompareOperator::NotIn,
                    syntax::CompareOperator::Is => hir::CompareOperator::Is,
                    syntax::CompareOperator::IsNot => hir::CompareOperator::IsNot,
                },
                left: Box::new(self.expression(left)?),
                right: Box::new(self.expression(right)?),
            },
            syntax::ExpressionKind::Call { callable, parts } => hir::ExpressionKind::Call {
                callable: Box::new(self.expression(callable)?),
                parts: parts
                    .iter()
                    .map(|part| match part {
                        syntax::CallPart::Positional(value) => {
                            Ok(hir::CallPart::Positional(self.expression(value)?))
                        }
                        syntax::CallPart::Starred(value) => {
                            Ok(hir::CallPart::Starred(self.expression(value)?))
                        }
                        syntax::CallPart::Keyword { name, value } => Ok(hir::CallPart::Keyword {
                            name: name.clone(),
                            value: self.expression(value)?,
                        }),
                        syntax::CallPart::KeywordUnpack(value) => {
                            Ok(hir::CallPart::KeywordUnpack(self.expression(value)?))
                        }
                    })
                    .collect::<Result<Vec<_>, DiagnosticSet>>()?,
            },
        };
        Ok(hir::Expression {
            span: expression.span,
            kind,
        })
    }

    fn binding(&self, name: &str) -> hir::Binding {
        if !self.plan.is_function || self.plan.explicit_globals.contains(name) {
            hir::Binding::Global
        } else if self.plan.cells.contains(name) {
            hir::Binding::Cell
        } else if self.plan.locals.contains(name) {
            hir::Binding::Local
        } else if self.plan.free.contains(name) {
            hir::Binding::Free
        } else {
            hir::Binding::Global
        }
    }
}

fn sema_error(path: &Path, message: impl Into<String>) -> DiagnosticSet {
    DiagnosticSet::one(Diagnostic::new(
        "RIM-SEMA-001",
        message,
        path,
        Span::default(),
    ))
}

fn capability_error(
    path: &Path,
    span: Span,
    code: &'static str,
    message: impl Into<String>,
) -> DiagnosticSet {
    DiagnosticSet::one(Diagnostic::new(code, message, path, span))
}

/// Class methods which call the builtin `super` without arguments need the
/// compiler-created `__class__` closure cell. This structural scan deliberately
/// ignores nested function definitions: their class-cell requirement belongs to
/// their own closure environment.
fn contains_zero_argument_super(statements: &[syntax::Statement]) -> bool {
    statements
        .iter()
        .any(statement_contains_zero_argument_super)
}

fn statement_contains_zero_argument_super(statement: &syntax::Statement) -> bool {
    match &statement.kind {
        syntax::StatementKind::Assign { targets, value } => {
            targets.iter().any(target_contains_zero_argument_super)
                || expression_contains_zero_argument_super(value)
        }
        syntax::StatementKind::AugAssign { target, value, .. } => {
            target_contains_zero_argument_super(target)
                || expression_contains_zero_argument_super(value)
        }
        syntax::StatementKind::Delete { targets } => {
            targets.iter().any(target_contains_zero_argument_super)
        }
        syntax::StatementKind::Expression(value)
        | syntax::StatementKind::Return { value: Some(value) } => {
            expression_contains_zero_argument_super(value)
        }
        syntax::StatementKind::Raise { exception, cause } => {
            exception
                .as_ref()
                .is_some_and(expression_contains_zero_argument_super)
                || cause
                    .as_ref()
                    .is_some_and(expression_contains_zero_argument_super)
        }
        syntax::StatementKind::Try {
            body,
            handlers,
            else_body,
            finally_body,
            ..
        } => {
            contains_zero_argument_super(body)
                || handlers.iter().any(|handler| {
                    handler
                        .exception_type
                        .as_ref()
                        .is_some_and(expression_contains_zero_argument_super)
                        || contains_zero_argument_super(&handler.body)
                })
                || contains_zero_argument_super(else_body)
                || contains_zero_argument_super(finally_body)
        }
        syntax::StatementKind::Print { values } => {
            values.iter().any(expression_contains_zero_argument_super)
        }
        syntax::StatementKind::If {
            condition,
            then_body,
            else_body,
        } => {
            expression_contains_zero_argument_super(condition)
                || contains_zero_argument_super(then_body)
                || contains_zero_argument_super(else_body)
        }
        syntax::StatementKind::While { condition, body } => {
            expression_contains_zero_argument_super(condition) || contains_zero_argument_super(body)
        }
        syntax::StatementKind::For {
            iterable,
            body,
            else_body,
            ..
        } => {
            expression_contains_zero_argument_super(iterable)
                || contains_zero_argument_super(body)
                || contains_zero_argument_super(else_body)
        }
        syntax::StatementKind::FunctionDef { .. }
        | syntax::StatementKind::ClassDef { .. }
        | syntax::StatementKind::Return { value: None }
        | syntax::StatementKind::Break
        | syntax::StatementKind::Continue
        | syntax::StatementKind::Global(_)
        | syntax::StatementKind::Nonlocal(_) => false,
    }
}

fn target_contains_zero_argument_super(target: &syntax::Target) -> bool {
    match &target.kind {
        syntax::TargetKind::Name(_) => false,
        syntax::TargetKind::Attribute { receiver, .. } => {
            expression_contains_zero_argument_super(receiver)
        }
        syntax::TargetKind::Item { collection, index } => {
            expression_contains_zero_argument_super(collection)
                || expression_contains_zero_argument_super(index)
        }
        syntax::TargetKind::Sequence { elements, .. } => {
            elements.iter().any(target_contains_zero_argument_super)
        }
        syntax::TargetKind::Starred(target) => target_contains_zero_argument_super(target),
    }
}

fn expression_contains_zero_argument_super(expression: &syntax::Expression) -> bool {
    match &expression.kind {
        syntax::ExpressionKind::Call { callable, parts } => {
            matches!(callable.kind, syntax::ExpressionKind::Name(ref name) if name == "super")
                && parts.is_empty()
                || expression_contains_zero_argument_super(callable)
                || parts.iter().any(|part| match part {
                    syntax::CallPart::Positional(value)
                    | syntax::CallPart::Starred(value)
                    | syntax::CallPart::Keyword { value, .. }
                    | syntax::CallPart::KeywordUnpack(value) => {
                        expression_contains_zero_argument_super(value)
                    }
                })
        }
        syntax::ExpressionKind::List(values)
        | syntax::ExpressionKind::Tuple(values)
        | syntax::ExpressionKind::Set(values)
        | syntax::ExpressionKind::Boolean { values, .. } => {
            values.iter().any(expression_contains_zero_argument_super)
        }
        syntax::ExpressionKind::Dictionary(values) => values.iter().any(|entry| match entry {
            syntax::DictionaryEntry::Pair { key, value } => {
                expression_contains_zero_argument_super(key)
                    || expression_contains_zero_argument_super(value)
            }
            syntax::DictionaryEntry::Unpack(value) => expression_contains_zero_argument_super(value),
        }),
        syntax::ExpressionKind::Subscript { value, index } => {
            expression_contains_zero_argument_super(value)
                || expression_contains_zero_argument_super(index)
        }
        syntax::ExpressionKind::Slice { start, stop, step } => [start, stop, step]
            .into_iter()
            .flatten()
            .any(|value| expression_contains_zero_argument_super(value)),
        syntax::ExpressionKind::Attribute { value, .. }
        | syntax::ExpressionKind::Unary { operand: value, .. } => {
            expression_contains_zero_argument_super(value)
        }
        syntax::ExpressionKind::Lambda { .. }
        | syntax::ExpressionKind::None
        | syntax::ExpressionKind::Bool(_)
        | syntax::ExpressionKind::Int(_)
        | syntax::ExpressionKind::Float(_)
        | syntax::ExpressionKind::String(_)
        | syntax::ExpressionKind::Bytes(_)
        | syntax::ExpressionKind::Complex { .. }
        | syntax::ExpressionKind::Name(_) => false,
        syntax::ExpressionKind::Binary { left, right, .. }
        | syntax::ExpressionKind::Compare { left, right, .. } => {
            expression_contains_zero_argument_super(left)
                || expression_contains_zero_argument_super(right)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path() -> &'static Path {
        Path::new("gate4_target.py")
    }

    fn resolve_first_assignment_target(source: &str) -> hir::Target {
        let module = syntax::parse(path(), source).unwrap();
        let raw = RawScope::module(&module.statements);
        let builtin_print_stable = !raw.mutates_global_name("print", true);
        let plan = resolve_scope(path(), raw, &[]).unwrap();
        let mut analyzer = Analyzer {
            path: path(),
            plan: &plan,
            child_index: 0,
            in_function: false,
            loop_depth: 0,
            in_except_star: false,
            handler_depth: 0,
            builtin_print_stable,
        };
        let syntax::StatementKind::Assign { targets, .. } = &module.statements[0].kind else {
            panic!("expected assignment");
        };
        analyzer.target(&targets[0]).unwrap()
    }

    #[test]
    fn recursive_target_tree_resolves_bindings_without_flattening() {
        let target = resolve_first_assignment_target("left, (middle, right), *tail = source\n");
        let hir::TargetKind::Sequence { elements, .. } = &target.kind else {
            panic!("expected outer sequence target");
        };
        assert_eq!(elements.len(), 3);
        assert!(matches!(
            elements[0].kind,
            hir::TargetKind::Name {
                ref name,
                binding: hir::Binding::Global
            } if name == "left"
        ));
        let hir::TargetKind::Sequence {
            elements: nested, ..
        } = &elements[1].kind
        else {
            panic!("expected nested target sequence");
        };
        assert_eq!(nested.len(), 2);
        assert!(
            matches!(nested[0].kind, hir::TargetKind::Name { ref name, binding: hir::Binding::Global } if name == "middle")
        );
        assert!(
            matches!(nested[1].kind, hir::TargetKind::Name { ref name, binding: hir::Binding::Global } if name == "right")
        );
        assert!(matches!(elements[2].kind, hir::TargetKind::Starred(_)));
    }

    #[test]
    fn nested_and_general_starred_assignment_targets_are_semantically_accepted() {
        for source in [
            "left, (middle, right) = source\n",
            "left, *middle, right = source\n",
            "*head, tail = source\n",
            "left, (inner, *rest), right = source\n",
        ] {
            let module = syntax::parse(path(), source).unwrap();
            analyze(path(), &module)
                .unwrap_or_else(|diagnostics| panic!("source: {source}: {diagnostics:?}"));
        }
    }

    #[test]
    fn recursive_loop_targets_share_the_assignment_target_contract() {
        for source in [
            "for left, right in source:\n    marker = left\n",
            "for left, *middle, right in source:\n    marker = middle\n",
            "for left, (middle, right) in source:\n    marker = right\n",
        ] {
            let module = syntax::parse(path(), source).unwrap();
            analyze(path(), &module)
                .unwrap_or_else(|diagnostics| panic!("source: {source}: {diagnostics:?}"));
        }
    }

    #[test]
    fn multiple_stars_at_one_target_level_are_rejected_before_mir() {
        let source = "left, *middle, *tail = source\n";
        let module = syntax::parse(path(), source).unwrap();
        let diagnostics = analyze(path(), &module).unwrap_err();
        let diagnostic = &diagnostics.as_slice()[0];
        assert_eq!(diagnostic.code, "RIM-CAP-G4-02");
        assert!(diagnostic.span.end > diagnostic.span.start);
    }
}
