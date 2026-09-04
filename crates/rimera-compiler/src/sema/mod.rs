use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use rimera_abi::RParameterKind;

use crate::core::{Diagnostic, DiagnosticSet, Span};
use crate::{hir, syntax};

fn is_pulled_forward_native_module(name: &str) -> bool {
    matches!(name, "inspect" | "weakref")
}

fn hir_type_parameters(parameters: &[syntax::TypeParameter]) -> Vec<hir::TypeParameter> {
    parameters
        .iter()
        .map(|parameter| hir::TypeParameter {
            span: parameter.span,
            name: parameter.name.clone(),
            kind: match parameter.kind {
                syntax::TypeParameterKind::TypeVar => hir::TypeParameterKind::TypeVar,
                syntax::TypeParameterKind::TypeVarTuple => hir::TypeParameterKind::TypeVarTuple,
                syntax::TypeParameterKind::ParamSpec => hir::TypeParameterKind::ParamSpec,
            },
        })
        .collect()
}

pub fn analyze(path: &Path, module: &syntax::Module) -> Result<hir::Module, DiagnosticSet> {
    let raw = RawScope::module(&module.statements);
    let builtin_print_stable = !raw.mutates_global_name("print", true);
    let dynamic_builtin_stable =
        ["eval", "exec", "compile"].map(|name| !raw.mutates_global_name(name, true));
    let plan = resolve_scope(path, raw, &[])?;
    let mut analyzer = Analyzer {
        path,
        plan: &plan,
        definition_type_params: BTreeSet::new(),
        child_index: 0,
        in_function: false,
        loop_depth: 0,
        in_except_star: false,
        handler_depth: 0,
        builtin_print_stable,
        dynamic_builtin_stable,
        class_depth: 0,
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
    is_class: bool,
    forced_free: BTreeSet<String>,
    parameters: BTreeSet<String>,
    assigned: BTreeSet<String>,
    local_order: Vec<String>,
    used: BTreeSet<String>,
    explicit_globals: BTreeSet<String>,
    nonlocals: BTreeSet<String>,
    global_spans: BTreeMap<String, Span>,
    nonlocal_spans: BTreeMap<String, Span>,
    children: Vec<RawScope>,
}

impl RawScope {
    fn module(statements: &[syntax::Statement]) -> Self {
        let mut scope = Self::empty(false, false);
        scope.scan_statements(statements);
        scope
    }

    fn function(
        parameters: &[syntax::Parameter],
        statements: &[syntax::Statement],
        type_params: &[syntax::TypeParameter],
    ) -> Self {
        let mut scope = Self::empty(true, false);
        scope
            .forced_free
            .extend(type_params.iter().map(|parameter| parameter.name.clone()));
        for parameter in parameters {
            scope.parameters.insert(parameter.name.clone());
            scope.local_order.push(parameter.name.clone());
        }
        scope.scan_statements(statements);
        scope
    }

    fn class(statements: &[syntax::Statement], type_params: &[syntax::TypeParameter]) -> Self {
        let mut scope = Self::empty(false, true);
        scope
            .forced_free
            .extend(type_params.iter().map(|parameter| parameter.name.clone()));
        scope.scan_class_statements(statements);
        scope
    }

    fn empty(is_function: bool, is_class: bool) -> Self {
        Self {
            is_function,
            is_class,
            forced_free: BTreeSet::new(),
            parameters: BTreeSet::new(),
            assigned: BTreeSet::new(),
            local_order: Vec::new(),
            used: BTreeSet::new(),
            explicit_globals: BTreeSet::new(),
            nonlocals: BTreeSet::new(),
            global_spans: BTreeMap::new(),
            nonlocal_spans: BTreeMap::new(),
            children: Vec::new(),
        }
    }

    fn record_assigned(&mut self, name: &str) {
        self.assigned.insert(name.to_owned());
        if !self.local_order.iter().any(|current| current == name) {
            self.local_order.push(name.to_owned());
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
                syntax::StatementKind::AnnAssign {
                    target,
                    annotation,
                    value,
                    ..
                } => {
                    self.scan_target(target);
                    if let Some(value) = value {
                        self.scan_expression(value);
                    }
                    if !self.is_function {
                        self.scan_expression(annotation);
                    }
                }
                syntax::StatementKind::Assert { test, message } => {
                    self.scan_expression(test);
                    if let Some(message) = message {
                        self.scan_expression(message);
                    }
                }
                syntax::StatementKind::FunctionDef {
                    name,
                    type_params,
                    decorators,
                    parameters,
                    return_annotation,
                    body,
                } => {
                    self.record_assigned(name);
                    for decorator in decorators {
                        self.scan_expression(decorator);
                    }
                    for parameter in parameters {
                        if let Some(default) = &parameter.default {
                            self.scan_expression(default);
                        }
                    }
                    for parameter in parameters {
                        if let Some(annotation) = &parameter.annotation {
                            self.scan_expression(annotation);
                        }
                    }
                    if let Some(annotation) = return_annotation {
                        self.scan_expression(annotation);
                    }
                    self.children
                        .push(Self::function(parameters, body, type_params));
                }
                syntax::StatementKind::ClassDef {
                    name,
                    type_params,
                    decorators,
                    bases,
                    metaclass,
                    keywords,
                    body,
                } => {
                    self.record_assigned(name);
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
                    self.children.push(Self::class(body, type_params));
                }
                syntax::StatementKind::TypeAlias {
                    name,
                    type_params: _,
                    value,
                } => {
                    self.record_assigned(name);
                    self.scan_expression(value);
                }
                syntax::StatementKind::Return { value } => {
                    if let Some(value) = value {
                        self.scan_expression(value);
                    }
                }
                syntax::StatementKind::Import { aliases } => {
                    for alias in aliases {
                        self.record_assigned(&alias.bind_name);
                    }
                }
                syntax::StatementKind::Break | syntax::StatementKind::Continue => {}
                syntax::StatementKind::Expression(value) => self.scan_expression(value),
                syntax::StatementKind::Global(names) => {
                    self.explicit_globals.extend(names.iter().cloned());
                    for name in names {
                        self.global_spans
                            .entry(name.clone())
                            .or_insert(statement.span);
                    }
                }
                syntax::StatementKind::Nonlocal(names) => {
                    self.nonlocals.extend(names.iter().cloned());
                    for name in names {
                        self.nonlocal_spans
                            .entry(name.clone())
                            .or_insert(statement.span);
                    }
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
                            self.record_assigned(name);
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
                syntax::StatementKind::Match { subject, cases } => {
                    self.scan_expression(subject);
                    for case in cases {
                        self.scan_pattern(&case.pattern);
                        if let Some(guard) = &case.guard {
                            self.scan_expression(guard);
                        }
                        self.scan_statements(&case.body);
                    }
                }
            }
        }
    }

    fn scan_pattern(&mut self, pattern: &syntax::Pattern) {
        match &pattern.kind {
            syntax::PatternKind::Value(value) => self.scan_expression(value),
            syntax::PatternKind::Capture(name) => {
                self.record_assigned(name);
            }
            syntax::PatternKind::As { pattern, name } => {
                self.scan_pattern(pattern);
                self.record_assigned(name);
            }
            syntax::PatternKind::Or(patterns) | syntax::PatternKind::Sequence(patterns) => {
                for pattern in patterns {
                    self.scan_pattern(pattern);
                }
            }
            syntax::PatternKind::Star(name) => {
                if let Some(name) = name {
                    self.record_assigned(name);
                }
            }
            syntax::PatternKind::Mapping {
                keys,
                patterns,
                rest,
            } => {
                for key in keys {
                    self.scan_expression(key);
                }
                for pattern in patterns {
                    self.scan_pattern(pattern);
                }
                if let Some(rest) = rest {
                    self.record_assigned(rest);
                }
            }
            syntax::PatternKind::Class {
                class,
                positional,
                keywords,
            } => {
                self.scan_expression(class);
                for pattern in positional {
                    self.scan_pattern(pattern);
                }
                for (_, pattern) in keywords {
                    self.scan_pattern(pattern);
                }
            }
            syntax::PatternKind::SingletonNone
            | syntax::PatternKind::SingletonBool(_)
            | syntax::PatternKind::Wildcard => {}
        }
    }

    fn scan_target(&mut self, target: &syntax::Target) {
        match &target.kind {
            syntax::TargetKind::Name(name) => {
                self.record_assigned(name);
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
            self.record_assigned(name);
            self.used.insert(name.clone());
        } else {
            self.scan_target_reads(target);
        }
    }

    fn scan_class_statements(&mut self, statements: &[syntax::Statement]) {
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
                syntax::StatementKind::AnnAssign {
                    target,
                    annotation,
                    value,
                    ..
                } => {
                    self.scan_target(target);
                    if let Some(value) = value {
                        self.scan_expression(value);
                    }
                    self.scan_expression(annotation);
                }
                syntax::StatementKind::Assert { test, message } => {
                    self.scan_expression(test);
                    if let Some(message) = message {
                        self.scan_expression(message);
                    }
                }
                syntax::StatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.scan_expression(condition);
                    self.scan_class_statements(then_body);
                    self.scan_class_statements(else_body);
                }
                syntax::StatementKind::While { condition, body } => {
                    self.scan_expression(condition);
                    self.scan_class_statements(body);
                }
                syntax::StatementKind::For {
                    target,
                    iterable,
                    body,
                    else_body,
                } => {
                    self.scan_target(target);
                    self.scan_expression(iterable);
                    self.scan_class_statements(body);
                    self.scan_class_statements(else_body);
                }
                syntax::StatementKind::Try {
                    body,
                    handlers,
                    else_body,
                    finally_body,
                    ..
                } => {
                    self.scan_class_statements(body);
                    for handler in handlers {
                        if let Some(exception_type) = &handler.exception_type {
                            self.scan_expression(exception_type);
                        }
                        if let Some(name) = &handler.name {
                            self.record_assigned(name);
                        }
                        self.scan_class_statements(&handler.body);
                    }
                    self.scan_class_statements(else_body);
                    self.scan_class_statements(finally_body);
                }
                syntax::StatementKind::Raise { exception, cause } => {
                    if let Some(exception) = exception {
                        self.scan_expression(exception);
                    }
                    if let Some(cause) = cause {
                        self.scan_expression(cause);
                    }
                }
                syntax::StatementKind::Expression(expression) => self.scan_expression(expression),
                syntax::StatementKind::Print { values } => {
                    values.iter().for_each(|value| self.scan_expression(value));
                }
                syntax::StatementKind::Global(names) => {
                    self.explicit_globals.extend(names.iter().cloned());
                    for name in names {
                        self.global_spans
                            .entry(name.clone())
                            .or_insert(statement.span);
                    }
                }
                syntax::StatementKind::Nonlocal(names) => {
                    self.nonlocals.extend(names.iter().cloned());
                    for name in names {
                        self.nonlocal_spans
                            .entry(name.clone())
                            .or_insert(statement.span);
                    }
                }
                syntax::StatementKind::FunctionDef {
                    name,
                    type_params,
                    decorators,
                    parameters,
                    return_annotation,
                    body,
                } => {
                    self.record_assigned(name);
                    for decorator in decorators {
                        self.scan_expression(decorator);
                    }
                    for parameter in parameters {
                        if let Some(default) = &parameter.default {
                            self.scan_expression(default);
                        }
                        if let Some(annotation) = &parameter.annotation {
                            self.scan_expression(annotation);
                        }
                    }
                    if let Some(annotation) = return_annotation {
                        self.scan_expression(annotation);
                    }
                    self.children
                        .push(Self::function(parameters, body, type_params));
                }
                syntax::StatementKind::ClassDef {
                    name,
                    type_params,
                    decorators,
                    bases,
                    metaclass,
                    keywords,
                    body,
                } => {
                    self.record_assigned(name);
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
                    self.children.push(Self::class(body, type_params));
                }
                syntax::StatementKind::Break | syntax::StatementKind::Continue => {}
                syntax::StatementKind::TypeAlias {
                    name,
                    type_params: _,
                    value,
                } => {
                    self.record_assigned(name);
                    self.scan_expression(value);
                }
                syntax::StatementKind::Return { value } => {
                    if let Some(value) = value {
                        self.scan_expression(value);
                    }
                }
                syntax::StatementKind::Import { aliases } => {
                    for alias in aliases {
                        self.record_assigned(&alias.bind_name);
                    }
                }
                syntax::StatementKind::Match { subject, cases } => {
                    self.scan_expression(subject);
                    for case in cases {
                        self.scan_pattern(&case.pattern);
                        if let Some(guard) = &case.guard {
                            self.scan_expression(guard);
                        }
                        self.scan_class_statements(&case.body);
                    }
                }
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
                let mut child = Self::empty(true, false);
                for parameter in parameters {
                    child.parameters.insert(parameter.name.clone());
                    child.local_order.push(parameter.name.clone());
                }
                child.scan_expression(body);
                self.children.push(child);
            }
            syntax::ExpressionKind::Unary { operand, .. } => self.scan_expression(operand),
            syntax::ExpressionKind::Boolean { values, .. } => {
                values.iter().for_each(|value| self.scan_expression(value));
            }
            syntax::ExpressionKind::Binary { left, right, .. } => {
                self.scan_expression(left);
                self.scan_expression(right);
            }
            syntax::ExpressionKind::Compare { left, comparisons } => {
                self.scan_expression(left);
                comparisons
                    .iter()
                    .for_each(|(_, right)| self.scan_expression(right));
            }
            syntax::ExpressionKind::NamedExpression { name, value } => {
                self.record_assigned(name);
                self.scan_expression(value);
            }
            syntax::ExpressionKind::Yield { value } => {
                if let Some(value) = value {
                    self.scan_expression(value);
                }
            }
            syntax::ExpressionKind::YieldFrom { value } => self.scan_expression(value),
            syntax::ExpressionKind::Comprehension {
                element,
                key,
                clauses,
                ..
            } => self.scan_comprehension(element, key.as_deref(), clauses),
            syntax::ExpressionKind::JoinedString(values) => {
                values.iter().for_each(|value| self.scan_expression(value));
            }
            syntax::ExpressionKind::FormattedValue {
                value, format_spec, ..
            } => {
                self.scan_expression(value);
                if let Some(format_spec) = format_spec {
                    self.scan_expression(format_spec);
                }
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

    fn scan_comprehension(
        &mut self,
        element: &syntax::Expression,
        key: Option<&syntax::Expression>,
        clauses: &[syntax::ComprehensionClause],
    ) {
        let Some((first, _)) = clauses.split_first() else {
            return;
        };

        // Python evaluates the outermost iterable in the containing scope and
        // only then enters the implicit comprehension function.
        self.scan_expression(&first.iterable);

        let mut walrus = BTreeSet::new();
        collect_comprehension_walrus_names(element, &mut walrus);
        if let Some(key) = key {
            collect_comprehension_walrus_names(key, &mut walrus);
        }
        for clause in clauses {
            collect_comprehension_walrus_names(&clause.iterable, &mut walrus);
            for filter in &clause.filters {
                collect_comprehension_walrus_names(filter, &mut walrus);
            }
        }
        for name in &walrus {
            self.record_assigned(name);
        }
        let globals = walrus
            .iter()
            .filter(|name| !self.is_function || self.explicit_globals.contains(*name))
            .cloned()
            .collect::<BTreeSet<_>>();
        let child = Self::comprehension_scope(element, key, clauses, &walrus, &globals);
        self.children.push(child);
    }

    fn comprehension_scope(
        element: &syntax::Expression,
        key: Option<&syntax::Expression>,
        clauses: &[syntax::ComprehensionClause],
        walrus: &BTreeSet<String>,
        globals: &BTreeSet<String>,
    ) -> Self {
        let mut scope = Self::empty(true, false);
        scope.parameters.insert(".0".to_owned());
        scope.local_order.push(".0".to_owned());
        for (index, clause) in clauses.iter().enumerate() {
            if index != 0 {
                scope.scan_expression_in_comprehension(&clause.iterable, walrus, globals);
            }
            scope.scan_target(&clause.target);
            for filter in &clause.filters {
                scope.scan_expression_in_comprehension(filter, walrus, globals);
            }
        }
        if let Some(key) = key {
            scope.scan_expression_in_comprehension(key, walrus, globals);
        }
        scope.scan_expression_in_comprehension(element, walrus, globals);
        scope
    }

    fn scan_expression_in_comprehension(
        &mut self,
        expression: &syntax::Expression,
        walrus: &BTreeSet<String>,
        globals: &BTreeSet<String>,
    ) {
        match &expression.kind {
            syntax::ExpressionKind::NamedExpression { name, value } if walrus.contains(name) => {
                self.used.insert(name.clone());
                if globals.contains(name) {
                    self.explicit_globals.insert(name.clone());
                } else {
                    self.nonlocals.insert(name.clone());
                }
                self.scan_expression_in_comprehension(value, walrus, globals);
            }
            syntax::ExpressionKind::Comprehension {
                element,
                key,
                clauses,
                ..
            } => {
                if let Some(first) = clauses.first() {
                    self.scan_expression_in_comprehension(&first.iterable, walrus, globals);
                    let child = Self::comprehension_scope(
                        element,
                        key.as_deref(),
                        clauses,
                        walrus,
                        globals,
                    );
                    self.children.push(child);
                }
            }
            syntax::ExpressionKind::Lambda { parameters, body } => {
                for parameter in parameters {
                    if let Some(default) = &parameter.default {
                        self.scan_expression_in_comprehension(default, walrus, globals);
                    }
                }
                let mut child = Self::empty(true, false);
                for parameter in parameters {
                    child.parameters.insert(parameter.name.clone());
                    child.local_order.push(parameter.name.clone());
                }
                child.scan_expression(body);
                self.children.push(child);
            }
            syntax::ExpressionKind::Name(name) => {
                self.used.insert(name.clone());
            }
            syntax::ExpressionKind::List(values)
            | syntax::ExpressionKind::Tuple(values)
            | syntax::ExpressionKind::Set(values)
            | syntax::ExpressionKind::Boolean { values, .. }
            | syntax::ExpressionKind::JoinedString(values) => {
                for value in values {
                    self.scan_expression_in_comprehension(value, walrus, globals);
                }
            }
            syntax::ExpressionKind::Dictionary(entries) => {
                for entry in entries {
                    match entry {
                        syntax::DictionaryEntry::Pair { key, value } => {
                            self.scan_expression_in_comprehension(key, walrus, globals);
                            self.scan_expression_in_comprehension(value, walrus, globals);
                        }
                        syntax::DictionaryEntry::Unpack(value) => {
                            self.scan_expression_in_comprehension(value, walrus, globals);
                        }
                    }
                }
            }
            syntax::ExpressionKind::Subscript { value, index } => {
                self.scan_expression_in_comprehension(value, walrus, globals);
                self.scan_expression_in_comprehension(index, walrus, globals);
            }
            syntax::ExpressionKind::Slice { start, stop, step } => {
                for value in [start, stop, step].into_iter().flatten() {
                    self.scan_expression_in_comprehension(value, walrus, globals);
                }
            }
            syntax::ExpressionKind::Attribute { value, .. }
            | syntax::ExpressionKind::Unary { operand: value, .. } => {
                self.scan_expression_in_comprehension(value, walrus, globals);
            }
            syntax::ExpressionKind::Binary { left, right, .. } => {
                self.scan_expression_in_comprehension(left, walrus, globals);
                self.scan_expression_in_comprehension(right, walrus, globals);
            }
            syntax::ExpressionKind::Compare { left, comparisons } => {
                self.scan_expression_in_comprehension(left, walrus, globals);
                for (_, right) in comparisons {
                    self.scan_expression_in_comprehension(right, walrus, globals);
                }
            }
            syntax::ExpressionKind::FormattedValue {
                value, format_spec, ..
            } => {
                self.scan_expression_in_comprehension(value, walrus, globals);
                if let Some(format_spec) = format_spec {
                    self.scan_expression_in_comprehension(format_spec, walrus, globals);
                }
            }
            syntax::ExpressionKind::Call { callable, parts } => {
                self.scan_expression_in_comprehension(callable, walrus, globals);
                for part in parts {
                    let value = match part {
                        syntax::CallPart::Positional(value)
                        | syntax::CallPart::Starred(value)
                        | syntax::CallPart::Keyword { value, .. }
                        | syntax::CallPart::KeywordUnpack(value) => value,
                    };
                    self.scan_expression_in_comprehension(value, walrus, globals);
                }
            }
            syntax::ExpressionKind::NamedExpression { name, value } => {
                self.record_assigned(name);
                self.scan_expression_in_comprehension(value, walrus, globals);
            }
            syntax::ExpressionKind::Yield { value } => {
                if let Some(value) = value {
                    self.scan_expression_in_comprehension(value, walrus, globals);
                }
            }
            syntax::ExpressionKind::YieldFrom { value } => {
                self.scan_expression_in_comprehension(value, walrus, globals);
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

fn collect_comprehension_walrus_names(
    expression: &syntax::Expression,
    names: &mut BTreeSet<String>,
) {
    match &expression.kind {
        syntax::ExpressionKind::NamedExpression { name, value } => {
            names.insert(name.clone());
            collect_comprehension_walrus_names(value, names);
        }
        syntax::ExpressionKind::Comprehension {
            element,
            key,
            clauses,
            ..
        } => {
            for clause in clauses {
                collect_comprehension_walrus_names(&clause.iterable, names);
                for filter in &clause.filters {
                    collect_comprehension_walrus_names(filter, names);
                }
            }
            if let Some(key) = key {
                collect_comprehension_walrus_names(key, names);
            }
            collect_comprehension_walrus_names(element, names);
        }
        syntax::ExpressionKind::Lambda { parameters, .. } => {
            for parameter in parameters {
                if let Some(default) = &parameter.default {
                    collect_comprehension_walrus_names(default, names);
                }
            }
        }
        syntax::ExpressionKind::List(values)
        | syntax::ExpressionKind::Tuple(values)
        | syntax::ExpressionKind::Set(values)
        | syntax::ExpressionKind::Boolean { values, .. }
        | syntax::ExpressionKind::JoinedString(values) => {
            for value in values {
                collect_comprehension_walrus_names(value, names);
            }
        }
        syntax::ExpressionKind::Dictionary(entries) => {
            for entry in entries {
                match entry {
                    syntax::DictionaryEntry::Pair { key, value } => {
                        collect_comprehension_walrus_names(key, names);
                        collect_comprehension_walrus_names(value, names);
                    }
                    syntax::DictionaryEntry::Unpack(value) => {
                        collect_comprehension_walrus_names(value, names);
                    }
                }
            }
        }
        syntax::ExpressionKind::Subscript { value, index } => {
            collect_comprehension_walrus_names(value, names);
            collect_comprehension_walrus_names(index, names);
        }
        syntax::ExpressionKind::Slice { start, stop, step } => {
            for value in [start, stop, step].into_iter().flatten() {
                collect_comprehension_walrus_names(value, names);
            }
        }
        syntax::ExpressionKind::Attribute { value, .. }
        | syntax::ExpressionKind::Unary { operand: value, .. } => {
            collect_comprehension_walrus_names(value, names);
        }
        syntax::ExpressionKind::Binary { left, right, .. } => {
            collect_comprehension_walrus_names(left, names);
            collect_comprehension_walrus_names(right, names);
        }
        syntax::ExpressionKind::Compare { left, comparisons } => {
            collect_comprehension_walrus_names(left, names);
            for (_, right) in comparisons {
                collect_comprehension_walrus_names(right, names);
            }
        }
        syntax::ExpressionKind::FormattedValue {
            value, format_spec, ..
        } => {
            collect_comprehension_walrus_names(value, names);
            if let Some(format_spec) = format_spec {
                collect_comprehension_walrus_names(format_spec, names);
            }
        }
        syntax::ExpressionKind::Call { callable, parts } => {
            collect_comprehension_walrus_names(callable, names);
            for part in parts {
                let value = match part {
                    syntax::CallPart::Positional(value)
                    | syntax::CallPart::Starred(value)
                    | syntax::CallPart::Keyword { value, .. }
                    | syntax::CallPart::KeywordUnpack(value) => value,
                };
                collect_comprehension_walrus_names(value, names);
            }
        }
        syntax::ExpressionKind::Yield { value } => {
            if let Some(value) = value {
                collect_comprehension_walrus_names(value, names);
            }
        }
        syntax::ExpressionKind::YieldFrom { value } => {
            collect_comprehension_walrus_names(value, names);
        }
        syntax::ExpressionKind::Name(_)
        | syntax::ExpressionKind::None
        | syntax::ExpressionKind::Bool(_)
        | syntax::ExpressionKind::Int(_)
        | syntax::ExpressionKind::Float(_)
        | syntax::ExpressionKind::String(_)
        | syntax::ExpressionKind::Bytes(_)
        | syntax::ExpressionKind::Complex { .. } => {}
    }
}

fn comprehension_target_names(target: &syntax::Target, names: &mut BTreeSet<String>) {
    match &target.kind {
        syntax::TargetKind::Name(name) => {
            names.insert(name.clone());
        }
        syntax::TargetKind::Sequence { elements, .. } => {
            for element in elements {
                comprehension_target_names(element, names);
            }
        }
        syntax::TargetKind::Starred(target) => comprehension_target_names(target, names),
        syntax::TargetKind::Attribute { .. } | syntax::TargetKind::Item { .. } => {}
    }
}

fn validate_comprehension_expression(
    path: &Path,
    expression: &syntax::Expression,
    iteration_names: &BTreeSet<String>,
    in_iterable: bool,
    in_class_body: bool,
) -> Result<(), DiagnosticSet> {
    match &expression.kind {
        syntax::ExpressionKind::NamedExpression { name, value } => {
            if in_iterable {
                return Err(sema_error(
                    path,
                    "assignment expression cannot be used in a comprehension iterable expression",
                ));
            }
            if iteration_names.contains(name) {
                return Err(sema_error(
                    path,
                    format!(
                        "assignment expression cannot rebind comprehension iteration variable `{name}`"
                    ),
                ));
            }
            if in_class_body {
                return Err(sema_error(
                    path,
                    "assignment expression within a comprehension cannot be used in a class body",
                ));
            }
            validate_comprehension_expression(
                path,
                value,
                iteration_names,
                in_iterable,
                in_class_body,
            )?;
        }
        syntax::ExpressionKind::Comprehension {
            element,
            key,
            clauses,
            ..
        } => {
            let mut nested_names = iteration_names.clone();
            for clause in clauses {
                comprehension_target_names(&clause.target, &mut nested_names);
            }
            for clause in clauses {
                validate_comprehension_expression(
                    path,
                    &clause.iterable,
                    &nested_names,
                    true,
                    in_class_body,
                )?;
                for filter in &clause.filters {
                    validate_comprehension_expression(
                        path,
                        filter,
                        &nested_names,
                        false,
                        in_class_body,
                    )?;
                }
            }
            if let Some(key) = key {
                validate_comprehension_expression(path, key, &nested_names, false, in_class_body)?;
            }
            validate_comprehension_expression(path, element, &nested_names, false, in_class_body)?;
        }
        syntax::ExpressionKind::Lambda { parameters, .. } => {
            for parameter in parameters {
                if let Some(default) = &parameter.default {
                    validate_comprehension_expression(
                        path,
                        default,
                        iteration_names,
                        in_iterable,
                        in_class_body,
                    )?;
                }
            }
        }
        syntax::ExpressionKind::List(values)
        | syntax::ExpressionKind::Tuple(values)
        | syntax::ExpressionKind::Set(values)
        | syntax::ExpressionKind::Boolean { values, .. }
        | syntax::ExpressionKind::JoinedString(values) => {
            for value in values {
                validate_comprehension_expression(
                    path,
                    value,
                    iteration_names,
                    in_iterable,
                    in_class_body,
                )?;
            }
        }
        syntax::ExpressionKind::Dictionary(entries) => {
            for entry in entries {
                match entry {
                    syntax::DictionaryEntry::Pair { key, value } => {
                        validate_comprehension_expression(
                            path,
                            key,
                            iteration_names,
                            in_iterable,
                            in_class_body,
                        )?;
                        validate_comprehension_expression(
                            path,
                            value,
                            iteration_names,
                            in_iterable,
                            in_class_body,
                        )?;
                    }
                    syntax::DictionaryEntry::Unpack(value) => {
                        validate_comprehension_expression(
                            path,
                            value,
                            iteration_names,
                            in_iterable,
                            in_class_body,
                        )?;
                    }
                }
            }
        }
        syntax::ExpressionKind::Subscript { value, index } => {
            validate_comprehension_expression(
                path,
                value,
                iteration_names,
                in_iterable,
                in_class_body,
            )?;
            validate_comprehension_expression(
                path,
                index,
                iteration_names,
                in_iterable,
                in_class_body,
            )?;
        }
        syntax::ExpressionKind::Slice { start, stop, step } => {
            for value in [start, stop, step].into_iter().flatten() {
                validate_comprehension_expression(
                    path,
                    value,
                    iteration_names,
                    in_iterable,
                    in_class_body,
                )?;
            }
        }
        syntax::ExpressionKind::Attribute { value, .. }
        | syntax::ExpressionKind::Unary { operand: value, .. } => {
            validate_comprehension_expression(
                path,
                value,
                iteration_names,
                in_iterable,
                in_class_body,
            )?;
        }
        syntax::ExpressionKind::Binary { left, right, .. } => {
            validate_comprehension_expression(
                path,
                left,
                iteration_names,
                in_iterable,
                in_class_body,
            )?;
            validate_comprehension_expression(
                path,
                right,
                iteration_names,
                in_iterable,
                in_class_body,
            )?;
        }
        syntax::ExpressionKind::Compare { left, comparisons } => {
            validate_comprehension_expression(
                path,
                left,
                iteration_names,
                in_iterable,
                in_class_body,
            )?;
            for (_, right) in comparisons {
                validate_comprehension_expression(
                    path,
                    right,
                    iteration_names,
                    in_iterable,
                    in_class_body,
                )?;
            }
        }
        syntax::ExpressionKind::FormattedValue {
            value, format_spec, ..
        } => {
            validate_comprehension_expression(
                path,
                value,
                iteration_names,
                in_iterable,
                in_class_body,
            )?;
            if let Some(format_spec) = format_spec {
                validate_comprehension_expression(
                    path,
                    format_spec,
                    iteration_names,
                    in_iterable,
                    in_class_body,
                )?;
            }
        }
        syntax::ExpressionKind::Call { callable, parts } => {
            validate_comprehension_expression(
                path,
                callable,
                iteration_names,
                in_iterable,
                in_class_body,
            )?;
            for part in parts {
                let value = match part {
                    syntax::CallPart::Positional(value)
                    | syntax::CallPart::Starred(value)
                    | syntax::CallPart::Keyword { value, .. }
                    | syntax::CallPart::KeywordUnpack(value) => value,
                };
                validate_comprehension_expression(
                    path,
                    value,
                    iteration_names,
                    in_iterable,
                    in_class_body,
                )?;
            }
        }
        syntax::ExpressionKind::Yield { value } => {
            if let Some(value) = value {
                validate_comprehension_expression(
                    path,
                    value,
                    iteration_names,
                    in_iterable,
                    in_class_body,
                )?;
            }
        }
        syntax::ExpressionKind::YieldFrom { value } => {
            validate_comprehension_expression(
                path,
                value,
                iteration_names,
                in_iterable,
                in_class_body,
            )?;
        }
        syntax::ExpressionKind::Name(_)
        | syntax::ExpressionKind::None
        | syntax::ExpressionKind::Bool(_)
        | syntax::ExpressionKind::Int(_)
        | syntax::ExpressionKind::Float(_)
        | syntax::ExpressionKind::String(_)
        | syntax::ExpressionKind::Bytes(_)
        | syntax::ExpressionKind::Complex { .. } => {}
    }
    Ok(())
}

#[derive(Debug)]
struct ScopePlan {
    is_function: bool,
    is_class: bool,
    forced_free: BTreeSet<String>,
    locals: BTreeSet<String>,
    local_order: Vec<String>,
    cells: BTreeSet<String>,
    free: BTreeSet<String>,
    explicit_globals: BTreeSet<String>,
    nonlocals: BTreeSet<String>,
    children: Vec<ScopePlan>,
}

fn resolve_scope(
    path: &Path,
    raw: RawScope,
    ancestors: &[BTreeSet<String>],
) -> Result<ScopePlan, DiagnosticSet> {
    if let Some(name) = raw.explicit_globals.intersection(&raw.nonlocals).next() {
        let span = raw
            .nonlocal_spans
            .get(name)
            .or_else(|| raw.global_spans.get(name))
            .copied()
            .unwrap_or_default();
        return Err(sema_error_at(
            path,
            span,
            "a name cannot be declared both global and nonlocal",
        ));
    }
    if let Some(name) = raw.parameters.intersection(&raw.explicit_globals).next() {
        let span = raw.global_spans.get(name).copied().unwrap_or_default();
        return Err(sema_error_at(
            path,
            span,
            "a parameter cannot be declared global or nonlocal",
        ));
    }
    if let Some(name) = raw.parameters.intersection(&raw.nonlocals).next() {
        let span = raw.nonlocal_spans.get(name).copied().unwrap_or_default();
        return Err(sema_error_at(
            path,
            span,
            "a parameter cannot be declared global or nonlocal",
        ));
    }
    if !raw.is_function && !raw.is_class && !raw.nonlocals.is_empty() {
        let span = raw
            .nonlocals
            .iter()
            .next()
            .and_then(|name| raw.nonlocal_spans.get(name))
            .copied()
            .unwrap_or_default();
        return Err(sema_error_at(
            path,
            span,
            "nonlocal declaration is not allowed at module scope",
        ));
    }

    let mut locals = raw.parameters.clone();
    locals.extend(raw.assigned.iter().cloned());
    locals.retain(|name| !raw.explicit_globals.contains(name) && !raw.nonlocals.contains(name));
    for name in &raw.nonlocals {
        if !ancestors.iter().rev().any(|scope| scope.contains(name)) {
            return Err(sema_error_at(
                path,
                raw.nonlocal_spans.get(name).copied().unwrap_or_default(),
                format!("no binding for nonlocal `{name}` was found"),
            ));
        }
    }

    let local_order = raw
        .local_order
        .iter()
        .filter(|name| locals.contains(*name))
        .cloned()
        .collect::<Vec<_>>();

    let mut free = raw.forced_free.clone();
    if raw.is_function || raw.is_class {
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
    } else if raw.is_class {
        child_ancestors.push(BTreeSet::from(["__class__".to_owned()]));
    }
    let mut children = Vec::with_capacity(raw.children.len());
    let mut cells = BTreeSet::new();
    for child in raw.children {
        let child = resolve_scope(path, child, &child_ancestors)?;
        for name in &child.free {
            if child.forced_free.contains(name) {
                continue;
            }
            if raw.is_function && locals.contains(name) {
                cells.insert(name.clone());
            } else if (raw.is_function || raw.is_class)
                && ancestors.iter().rev().any(|scope| scope.contains(name))
            {
                free.insert(name.clone());
            }
        }
        children.push(child);
    }
    Ok(ScopePlan {
        is_function: raw.is_function,
        is_class: raw.is_class,
        forced_free: raw.forced_free,
        locals,
        local_order,
        cells,
        free,
        explicit_globals: raw.explicit_globals,
        nonlocals: raw.nonlocals,
        children,
    })
}

struct Analyzer<'a> {
    path: &'a Path,
    plan: &'a ScopePlan,
    definition_type_params: BTreeSet<String>,
    child_index: usize,
    in_function: bool,
    loop_depth: usize,
    in_except_star: bool,
    handler_depth: usize,
    builtin_print_stable: bool,
    dynamic_builtin_stable: [bool; 3],
    class_depth: usize,
}

impl Analyzer<'_> {
    fn with_definition_type_params<T>(
        &mut self,
        parameters: &[syntax::TypeParameter],
        operation: impl FnOnce(&mut Self) -> Result<T, DiagnosticSet>,
    ) -> Result<T, DiagnosticSet> {
        let previous = std::mem::replace(
            &mut self.definition_type_params,
            parameters
                .iter()
                .map(|parameter| parameter.name.clone())
                .collect(),
        );
        let result = operation(self);
        self.definition_type_params = previous;
        result
    }

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
            syntax::StatementKind::AnnAssign {
                target,
                annotation,
                value,
                simple,
            } => {
                let target = self.target(target)?;
                self.validate_augmented_target(&target)?;
                hir::StatementKind::AnnAssign {
                    target,
                    annotation: self.expression(annotation)?,
                    value: value
                        .as_ref()
                        .map(|value| self.expression(value))
                        .transpose()?,
                    simple: *simple,
                }
            }
            syntax::StatementKind::Assert { test, message } => hir::StatementKind::Assert {
                test: self.expression(test)?,
                message: message
                    .as_ref()
                    .map(|message| self.expression(message))
                    .transpose()?,
            },
            syntax::StatementKind::FunctionDef {
                name,
                type_params,
                decorators,
                parameters,
                return_annotation,
                body,
            } => {
                let decorators = decorators
                    .iter()
                    .map(|decorator| self.expression(decorator))
                    .collect::<Result<Vec<_>, _>>()?;
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
                let (parameter_annotations, return_annotation) =
                    self.with_definition_type_params(type_params, |analyzer| {
                        let parameter_annotations = parameters
                            .iter()
                            .map(|parameter| {
                                parameter
                                    .annotation
                                    .as_ref()
                                    .map(|annotation| analyzer.expression(annotation))
                                    .transpose()
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        let return_annotation = return_annotation
                            .as_ref()
                            .map(|annotation| analyzer.expression(annotation))
                            .transpose()?;
                        Ok((parameter_annotations, return_annotation))
                    })?;
                let child = self
                    .plan
                    .children
                    .get(self.child_index)
                    .ok_or_else(|| sema_error(self.path, "function scope plan is missing"))?;
                self.child_index += 1;
                let mut analyzer = Analyzer {
                    path: self.path,
                    plan: child,
                    definition_type_params: BTreeSet::new(),
                    child_index: 0,
                    in_function: true,
                    loop_depth: 0,
                    in_except_star: false,
                    handler_depth: 0,
                    builtin_print_stable: self.builtin_print_stable,
                    dynamic_builtin_stable: self.dynamic_builtin_stable,
                    class_depth: 0,
                };
                let parameters = parameters
                    .iter()
                    .zip(defaults)
                    .zip(parameter_annotations)
                    .map(|((parameter, default), annotation)| hir::Parameter {
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
                        annotation,
                    })
                    .collect();
                hir::StatementKind::FunctionDef {
                    name: name.clone(),
                    binding: self.binding(name),
                    type_params: hir_type_parameters(type_params),
                    decorators,
                    parameters,
                    return_annotation,
                    body: analyzer.statements(body)?,
                    locals: child.local_order.clone(),
                    cells: child.cells.iter().cloned().collect(),
                    free: child.free.iter().cloned().collect(),
                }
            }
            syntax::StatementKind::ClassDef {
                name,
                type_params,
                decorators,
                bases,
                metaclass,
                keywords,
                body,
            } => hir::StatementKind::ClassDef {
                name: name.clone(),
                binding: self.binding(name),
                type_params: hir_type_parameters(type_params),
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
                body: self.class_body(body)?,
            },
            syntax::StatementKind::TypeAlias {
                name,
                type_params,
                value,
            } => {
                let value = self.with_definition_type_params(type_params, |analyzer| {
                    analyzer.expression(value)
                })?;
                hir::StatementKind::TypeAlias {
                    name: name.clone(),
                    binding: self.binding(name),
                    type_params: hir_type_parameters(type_params),
                    value,
                }
            }
            syntax::StatementKind::Import { aliases } => {
                for alias in aliases {
                    if !is_pulled_forward_native_module(&alias.module) {
                        return Err(capability_error(
                            self.path,
                            statement.span,
                            "RIM-CAP-001",
                            format!(
                                "module `{}` is not registered in the pulled-forward native import foundation",
                                alias.module
                            ),
                        ));
                    }
                }
                hir::StatementKind::Import {
                    aliases: aliases
                        .iter()
                        .map(|alias| hir::ImportAlias {
                            module: alias.module.clone(),
                            bind_name: alias.bind_name.clone(),
                            binding: self.binding(&alias.bind_name),
                        })
                        .collect(),
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
                if let syntax::ExpressionKind::Call { callable, parts } = &expression.kind
                    && parts
                        .iter()
                        .all(|part| matches!(part, syntax::CallPart::Positional(_)))
                    && self.builtin_print_stable
                    && self.binding("print") == hir::Binding::Global
                    && matches!(
                        &callable.kind,
                        syntax::ExpressionKind::Name(name) if name == "print"
                    )
                {
                    hir::StatementKind::Print {
                        values: parts
                            .iter()
                            .map(|part| match part {
                                syntax::CallPart::Positional(value) => self.expression(value),
                                _ => unreachable!("print fast path checked positional-only parts"),
                            })
                            .collect::<Result<_, _>>()?,
                    }
                } else {
                    hir::StatementKind::Expression(self.expression(expression)?)
                }
            }
            syntax::StatementKind::Raise { exception, cause } => {
                // Bare raise is syntactically valid outside a lexical handler.
                // The runtime handled-exception stack decides whether it reraises
                // an active exception or produces RuntimeError, including across
                // native function-call boundaries.
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
            syntax::StatementKind::Match { subject, cases } => hir::StatementKind::Match {
                subject: self.expression(subject)?,
                cases: cases
                    .iter()
                    .map(|case| {
                        Ok(hir::MatchCase {
                            pattern: self.pattern(&case.pattern)?,
                            guard: case
                                .guard
                                .as_ref()
                                .map(|guard| self.expression(guard))
                                .transpose()?,
                            body: self.statements(&case.body)?,
                        })
                    })
                    .collect::<Result<Vec<_>, DiagnosticSet>>()?,
            },
        };
        Ok(Some(hir::Statement {
            span: statement.span,
            kind,
        }))
    }

    fn class_body(
        &mut self,
        statements: &[syntax::Statement],
    ) -> Result<Vec<hir::ClassMember>, DiagnosticSet> {
        let child = self
            .plan
            .children
            .get(self.child_index)
            .ok_or_else(|| sema_error(self.path, "class scope plan is missing"))?;
        self.child_index += 1;
        if !child.is_class {
            return Err(sema_error(self.path, "class scope plan has the wrong kind"));
        }
        let mut analyzer = Analyzer {
            path: self.path,
            plan: child,
            definition_type_params: BTreeSet::new(),
            child_index: 0,
            in_function: false,
            loop_depth: 0,
            in_except_star: false,
            handler_depth: 0,
            builtin_print_stable: self.builtin_print_stable,
            dynamic_builtin_stable: self.dynamic_builtin_stable,
            class_depth: self.class_depth,
        };
        analyzer.class_members(statements)
    }

    fn class_members(
        &mut self,
        statements: &[syntax::Statement],
    ) -> Result<Vec<hir::ClassMember>, DiagnosticSet> {
        self.class_depth += 1;
        let result = self.class_members_inner(statements);
        self.class_depth -= 1;
        result
    }

    fn class_members_inner(
        &mut self,
        statements: &[syntax::Statement],
    ) -> Result<Vec<hir::ClassMember>, DiagnosticSet> {
        let mut members = Vec::with_capacity(statements.len());
        for statement in statements {
            if matches!(
                statement.kind,
                syntax::StatementKind::Global(_) | syntax::StatementKind::Nonlocal(_)
            ) {
                continue;
            }
            let member = match &statement.kind {
                syntax::StatementKind::Assign { targets, value } => hir::ClassMember::Assign {
                    targets: targets
                        .iter()
                        .map(|target| self.target(target))
                        .collect::<Result<Vec<_>, _>>()?,
                    value: self.expression(value)?,
                },
                syntax::StatementKind::AugAssign { target, op, value } => {
                    let target = self.target(target)?;
                    self.validate_augmented_target(&target)?;
                    hir::ClassMember::AugAssign {
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
                    hir::ClassMember::Delete { targets }
                }
                syntax::StatementKind::AnnAssign {
                    target,
                    annotation,
                    value,
                    simple,
                } => {
                    let target = self.target(target)?;
                    self.validate_augmented_target(&target)?;
                    hir::ClassMember::AnnAssign {
                        target,
                        annotation: self.expression(annotation)?,
                        value: value
                            .as_ref()
                            .map(|value| self.expression(value))
                            .transpose()?,
                        simple: *simple,
                    }
                }
                syntax::StatementKind::Assert { test, message } => hir::ClassMember::Assert {
                    test: self.expression(test)?,
                    message: message
                        .as_ref()
                        .map(|message| self.expression(message))
                        .transpose()?,
                },
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
                                name: handler
                                    .name
                                    .as_ref()
                                    .map(|name| (name.clone(), self.binding(name))),
                                body: self.class_members(&handler.body)?,
                            })
                        })
                        .collect::<Result<_, DiagnosticSet>>()?,
                    else_body: self.class_members(else_body)?,
                    finally_body: self.class_members(finally_body)?,
                    is_star: *is_star,
                },
                syntax::StatementKind::Import { aliases } => {
                    for alias in aliases {
                        if !is_pulled_forward_native_module(&alias.module) {
                            return Err(capability_error(
                                self.path,
                                statement.span,
                                "RIM-CAP-001",
                                format!(
                                    "module `{}` is not registered in the pulled-forward native import foundation",
                                    alias.module
                                ),
                            ));
                        }
                    }
                    hir::ClassMember::Import {
                        aliases: aliases
                            .iter()
                            .map(|alias| hir::ImportAlias {
                                module: alias.module.clone(),
                                bind_name: alias.bind_name.clone(),
                                binding: self.binding(&alias.bind_name),
                            })
                            .collect(),
                    }
                }
                syntax::StatementKind::ClassDef {
                    name,
                    type_params,
                    decorators,
                    bases,
                    metaclass,
                    keywords,
                    body,
                } => hir::ClassMember::ClassDef {
                    name: name.clone(),
                    binding: self.binding(name),
                    type_params: hir_type_parameters(type_params),
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
                    body: self.class_body(body)?,
                },
                syntax::StatementKind::TypeAlias {
                    name,
                    type_params,
                    value,
                } => {
                    let value = self.with_definition_type_params(type_params, |analyzer| {
                        analyzer.expression(value)
                    })?;
                    hir::ClassMember::TypeAlias {
                        name: name.clone(),
                        binding: self.binding(name),
                        type_params: hir_type_parameters(type_params),
                        value,
                    }
                }
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
                    type_params,
                    decorators,
                    parameters,
                    return_annotation,
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
                    let (parameter_annotations, return_annotation) = self
                        .with_definition_type_params(type_params, |analyzer| {
                            let parameter_annotations = parameters
                                .iter()
                                .map(|parameter| {
                                    parameter
                                        .annotation
                                        .as_ref()
                                        .map(|annotation| analyzer.expression(annotation))
                                        .transpose()
                                })
                                .collect::<Result<Vec<_>, _>>()?;
                            let return_annotation = return_annotation
                                .as_ref()
                                .map(|annotation| analyzer.expression(annotation))
                                .transpose()?;
                            Ok((parameter_annotations, return_annotation))
                        })?;
                    let child =
                        self.plan.children.get(self.child_index).ok_or_else(|| {
                            sema_error(self.path, "function scope plan is missing")
                        })?;
                    self.child_index += 1;
                    let mut analyzer = Analyzer {
                        path: self.path,
                        plan: child,
                        definition_type_params: BTreeSet::new(),
                        child_index: 0,
                        in_function: true,
                        loop_depth: 0,
                        in_except_star: false,
                        handler_depth: 0,
                        builtin_print_stable: self.builtin_print_stable,
                        dynamic_builtin_stable: self.dynamic_builtin_stable,
                        class_depth: 0,
                    };
                    let parameters = parameters
                        .iter()
                        .zip(defaults)
                        .zip(parameter_annotations)
                        .map(|((parameter, default), annotation)| hir::Parameter {
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
                            annotation,
                        })
                        .collect();
                    hir::ClassMember::FunctionDef {
                        span: statement.span,
                        name: name.clone(),
                        binding: self.binding(name),
                        type_params: hir_type_parameters(type_params),
                        decorators: decorators
                            .iter()
                            .map(|decorator| self.expression(decorator))
                            .collect::<Result<Vec<_>, _>>()?,
                        uses_zero_argument_super: contains_zero_argument_super(body),
                        parameters,
                        return_annotation,
                        body: analyzer.statements(body)?,
                        locals: child.local_order.clone(),
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
                        "RIM-SEMA-001",
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
                "RIM-SEMA-001",
                "invalid augmented-assignment target shape",
            )),
        }
    }

    fn validate_delete_target(&self, target: &hir::Target) -> Result<(), DiagnosticSet> {
        match &target.kind {
            hir::TargetKind::Name { .. }
            | hir::TargetKind::Attribute { .. }
            | hir::TargetKind::Item { .. } => Ok(()),
            hir::TargetKind::Sequence { elements, .. } => {
                for element in elements {
                    self.validate_delete_target(element)?;
                }
                Ok(())
            }
            hir::TargetKind::Starred(_) => Err(capability_error(
                self.path,
                target.span,
                "RIM-SEMA-001",
                "starred deletion targets are invalid",
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
                    definition_type_params: BTreeSet::new(),
                    child_index: 0,
                    in_function: true,
                    loop_depth: 0,
                    in_except_star: false,
                    handler_depth: 0,
                    builtin_print_stable: self.builtin_print_stable,
                    dynamic_builtin_stable: self.dynamic_builtin_stable,
                    class_depth: 0,
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
                        annotation: None,
                    })
                    .collect();
                hir::ExpressionKind::Lambda {
                    parameters,
                    body: Box::new(analyzer.expression(body)?),
                    locals: child.local_order.clone(),
                    cells: child.cells.iter().cloned().collect(),
                    free: child.free.iter().cloned().collect(),
                }
            }
            syntax::ExpressionKind::Name(name) => {
                let binding = self.binding(name);
                if let Some(index) = ["eval", "exec", "compile"]
                    .iter()
                    .position(|candidate| *candidate == name)
                    && self.dynamic_builtin_stable[index]
                    && matches!(binding, hir::Binding::Global)
                {
                    return Err(capability_error(
                        self.path,
                        expression.span,
                        "RIM-CAP-G7-02",
                        format!(
                            "dynamic Python compilation via `{name}` is not supported by Gate 7"
                        ),
                    ));
                }
                hir::ExpressionKind::Name {
                    name: name.clone(),
                    binding,
                }
            }
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
            syntax::ExpressionKind::Compare { left, comparisons } => hir::ExpressionKind::Compare {
                left: Box::new(self.expression(left)?),
                comparisons: comparisons
                    .iter()
                    .map(|(op, right)| {
                        let op = match op {
                            syntax::CompareOperator::Equal => hir::CompareOperator::Equal,
                            syntax::CompareOperator::NotEqual => hir::CompareOperator::NotEqual,
                            syntax::CompareOperator::Less => hir::CompareOperator::Less,
                            syntax::CompareOperator::LessEqual => hir::CompareOperator::LessEqual,
                            syntax::CompareOperator::Greater => hir::CompareOperator::Greater,
                            syntax::CompareOperator::GreaterEqual => {
                                hir::CompareOperator::GreaterEqual
                            }
                            syntax::CompareOperator::In => hir::CompareOperator::In,
                            syntax::CompareOperator::NotIn => hir::CompareOperator::NotIn,
                            syntax::CompareOperator::Is => hir::CompareOperator::Is,
                            syntax::CompareOperator::IsNot => hir::CompareOperator::IsNot,
                        };
                        Ok((op, self.expression(right)?))
                    })
                    .collect::<Result<Vec<_>, DiagnosticSet>>()?,
            },
            syntax::ExpressionKind::NamedExpression { name, value } => {
                hir::ExpressionKind::NamedExpression {
                    name: name.clone(),
                    binding: self.binding(name),
                    value: Box::new(self.expression(value)?),
                }
            }
            syntax::ExpressionKind::Yield { value } => {
                if !self.in_function {
                    return Err(sema_error_at(
                        self.path,
                        expression.span,
                        "`yield` is only valid inside a function",
                    ));
                }
                hir::ExpressionKind::Yield {
                    value: value
                        .as_deref()
                        .map(|value| self.expression(value))
                        .transpose()?
                        .map(Box::new),
                }
            }
            syntax::ExpressionKind::YieldFrom { value } => {
                if !self.in_function {
                    return Err(sema_error_at(
                        self.path,
                        expression.span,
                        "`yield from` is only valid inside a function",
                    ));
                }
                hir::ExpressionKind::YieldFrom {
                    value: Box::new(self.expression(value)?),
                }
            }
            syntax::ExpressionKind::Comprehension {
                kind,
                element,
                key,
                clauses,
            } => {
                let Some(first) = clauses.first() else {
                    return Err(sema_error(
                        self.path,
                        "comprehension has no generator clause",
                    ));
                };
                let mut iteration_names = BTreeSet::new();
                for clause in clauses {
                    comprehension_target_names(&clause.target, &mut iteration_names);
                }
                for clause in clauses {
                    validate_comprehension_expression(
                        self.path,
                        &clause.iterable,
                        &iteration_names,
                        true,
                        self.class_depth != 0,
                    )?;
                    for filter in &clause.filters {
                        validate_comprehension_expression(
                            self.path,
                            filter,
                            &iteration_names,
                            false,
                            self.class_depth != 0,
                        )?;
                    }
                }
                if let Some(key) = key {
                    validate_comprehension_expression(
                        self.path,
                        key,
                        &iteration_names,
                        false,
                        self.class_depth != 0,
                    )?;
                }
                validate_comprehension_expression(
                    self.path,
                    element,
                    &iteration_names,
                    false,
                    self.class_depth != 0,
                )?;

                let outer_iterable = self.expression(&first.iterable)?;
                let child =
                    self.plan.children.get(self.child_index).ok_or_else(|| {
                        sema_error(self.path, "comprehension scope plan is missing")
                    })?;
                self.child_index += 1;
                let mut analyzer = Analyzer {
                    path: self.path,
                    plan: child,
                    definition_type_params: BTreeSet::new(),
                    child_index: 0,
                    in_function: true,
                    loop_depth: 0,
                    in_except_star: false,
                    handler_depth: 0,
                    builtin_print_stable: self.builtin_print_stable,
                    dynamic_builtin_stable: self.dynamic_builtin_stable,
                    class_depth: 0,
                };
                let clauses = clauses
                    .iter()
                    .enumerate()
                    .map(|(index, clause)| {
                        let iterable = if index == 0 {
                            None
                        } else {
                            Some(analyzer.expression(&clause.iterable)?)
                        };
                        let target = analyzer.target(&clause.target)?;
                        analyzer.validate_loop_target(&target)?;
                        let filters = clause
                            .filters
                            .iter()
                            .map(|filter| analyzer.expression(filter))
                            .collect::<Result<Vec<_>, _>>()?;
                        Ok(hir::ComprehensionClause {
                            target,
                            iterable,
                            filters,
                        })
                    })
                    .collect::<Result<Vec<_>, DiagnosticSet>>()?;
                let key = key
                    .as_deref()
                    .map(|key| analyzer.expression(key))
                    .transpose()?
                    .map(Box::new);
                let element = Box::new(analyzer.expression(element)?);
                hir::ExpressionKind::Comprehension {
                    kind: match kind {
                        syntax::ComprehensionKind::List => hir::ComprehensionKind::List,
                        syntax::ComprehensionKind::Set => hir::ComprehensionKind::Set,
                        syntax::ComprehensionKind::Dictionary => hir::ComprehensionKind::Dictionary,
                        syntax::ComprehensionKind::Generator => hir::ComprehensionKind::Generator,
                    },
                    outer_iterable: Box::new(outer_iterable),
                    element,
                    key,
                    clauses,
                    locals: child
                        .locals
                        .iter()
                        .filter(|name| name.as_str() != ".0")
                        .cloned()
                        .collect(),
                    cells: child.cells.iter().cloned().collect(),
                    free: child.free.iter().cloned().collect(),
                }
            }
            syntax::ExpressionKind::JoinedString(values) => hir::ExpressionKind::JoinedString(
                values
                    .iter()
                    .map(|value| self.expression(value))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            syntax::ExpressionKind::FormattedValue {
                value,
                conversion,
                format_spec,
            } => hir::ExpressionKind::FormattedValue {
                value: Box::new(self.expression(value)?),
                conversion: match conversion {
                    syntax::FormatConversion::None => hir::FormatConversion::None,
                    syntax::FormatConversion::Str => hir::FormatConversion::Str,
                    syntax::FormatConversion::Repr => hir::FormatConversion::Repr,
                    syntax::FormatConversion::Ascii => hir::FormatConversion::Ascii,
                },
                format_spec: format_spec
                    .as_deref()
                    .map(|spec| self.expression(spec))
                    .transpose()?
                    .map(Box::new),
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

    fn pattern(&mut self, pattern: &syntax::Pattern) -> Result<hir::Pattern, DiagnosticSet> {
        pattern_capture_set(self.path, pattern)?;
        self.pattern_inner(pattern)
    }

    fn pattern_inner(&mut self, pattern: &syntax::Pattern) -> Result<hir::Pattern, DiagnosticSet> {
        let kind = match &pattern.kind {
            syntax::PatternKind::Value(value) => hir::PatternKind::Value(self.expression(value)?),
            syntax::PatternKind::SingletonNone => hir::PatternKind::SingletonNone,
            syntax::PatternKind::SingletonBool(value) => hir::PatternKind::SingletonBool(*value),
            syntax::PatternKind::Capture(name) => hir::PatternKind::Capture {
                name: name.clone(),
                binding: self.binding(name),
            },
            syntax::PatternKind::Wildcard => hir::PatternKind::Wildcard,
            syntax::PatternKind::As { pattern, name } => hir::PatternKind::As {
                pattern: Box::new(self.pattern_inner(pattern)?),
                name: name.clone(),
                binding: self.binding(name),
            },
            syntax::PatternKind::Or(patterns) => hir::PatternKind::Or(
                patterns
                    .iter()
                    .map(|pattern| self.pattern_inner(pattern))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            syntax::PatternKind::Sequence(patterns) => hir::PatternKind::Sequence(
                patterns
                    .iter()
                    .map(|pattern| self.pattern_inner(pattern))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            syntax::PatternKind::Star(name) => {
                hir::PatternKind::Star(name.as_ref().map(|name| (name.clone(), self.binding(name))))
            }
            syntax::PatternKind::Mapping {
                keys,
                patterns,
                rest,
            } => hir::PatternKind::Mapping {
                keys: keys
                    .iter()
                    .map(|key| self.expression(key))
                    .collect::<Result<Vec<_>, _>>()?,
                patterns: patterns
                    .iter()
                    .map(|pattern| self.pattern_inner(pattern))
                    .collect::<Result<Vec<_>, _>>()?,
                rest: rest.as_ref().map(|name| (name.clone(), self.binding(name))),
            },
            syntax::PatternKind::Class {
                class,
                positional,
                keywords,
            } => hir::PatternKind::Class {
                class: self.expression(class)?,
                positional: positional
                    .iter()
                    .map(|pattern| self.pattern_inner(pattern))
                    .collect::<Result<Vec<_>, _>>()?,
                keywords: keywords
                    .iter()
                    .map(|(name, pattern)| Ok((name.clone(), self.pattern_inner(pattern)?)))
                    .collect::<Result<Vec<_>, DiagnosticSet>>()?,
            },
        };
        Ok(hir::Pattern {
            span: pattern.span,
            kind,
        })
    }

    fn binding(&self, name: &str) -> hir::Binding {
        if self.definition_type_params.contains(name) {
            hir::Binding::Free
        } else if self.plan.is_class {
            if self.plan.explicit_globals.contains(name) {
                hir::Binding::Global
            } else if self.plan.nonlocals.contains(name) {
                hir::Binding::Free
            } else if self.plan.free.contains(name) {
                hir::Binding::ClassFree
            } else {
                hir::Binding::ClassName
            }
        } else if !self.plan.is_function || self.plan.explicit_globals.contains(name) {
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

fn pattern_capture_set(
    path: &Path,
    pattern: &syntax::Pattern,
) -> Result<BTreeSet<String>, DiagnosticSet> {
    match &pattern.kind {
        syntax::PatternKind::Value(_)
        | syntax::PatternKind::SingletonNone
        | syntax::PatternKind::SingletonBool(_)
        | syntax::PatternKind::Wildcard => Ok(BTreeSet::new()),
        syntax::PatternKind::Capture(name) => Ok(BTreeSet::from([name.clone()])),
        syntax::PatternKind::As {
            pattern: inner,
            name,
        } => {
            let mut captures = pattern_capture_set(path, inner)?;
            if !captures.insert(name.clone()) {
                return Err(DiagnosticSet::one(Diagnostic::new(
                    "RIM-SEMA-001",
                    format!("pattern captures `{name}` more than once"),
                    path,
                    pattern.span,
                )));
            }
            Ok(captures)
        }
        syntax::PatternKind::Or(patterns) => {
            let mut expected: Option<BTreeSet<String>> = None;
            for (index, alternative) in patterns.iter().enumerate() {
                if index + 1 < patterns.len() && pattern_is_irrefutable(alternative) {
                    return Err(DiagnosticSet::one(Diagnostic::new(
                        "RIM-SEMA-001",
                        "irrefutable OR-pattern alternative makes later alternatives unreachable",
                        path,
                        alternative.span,
                    )));
                }
                let captures = pattern_capture_set(path, alternative)?;
                if let Some(expected) = &expected {
                    if expected != &captures {
                        return Err(DiagnosticSet::one(Diagnostic::new(
                            "RIM-SEMA-001",
                            "OR-pattern alternatives must bind the same names",
                            path,
                            pattern.span,
                        )));
                    }
                } else {
                    expected = Some(captures);
                }
            }
            Ok(expected.unwrap_or_default())
        }
        syntax::PatternKind::Sequence(patterns) => {
            merge_pattern_capture_sets(path, pattern.span, patterns.iter(), None)
        }
        syntax::PatternKind::Star(name) => Ok(name.iter().cloned().collect()),
        syntax::PatternKind::Mapping { patterns, rest, .. } => {
            merge_pattern_capture_sets(path, pattern.span, patterns.iter(), rest.as_ref())
        }
        syntax::PatternKind::Class {
            positional,
            keywords,
            ..
        } => merge_pattern_capture_sets(
            path,
            pattern.span,
            positional
                .iter()
                .chain(keywords.iter().map(|(_, pattern)| pattern)),
            None,
        ),
    }
}

fn merge_pattern_capture_sets<'a>(
    path: &Path,
    span: Span,
    patterns: impl Iterator<Item = &'a syntax::Pattern>,
    extra: Option<&String>,
) -> Result<BTreeSet<String>, DiagnosticSet> {
    let mut captures = BTreeSet::new();
    for child in patterns {
        for name in pattern_capture_set(path, child)? {
            if !captures.insert(name.clone()) {
                return Err(DiagnosticSet::one(Diagnostic::new(
                    "RIM-SEMA-001",
                    format!("pattern captures `{name}` more than once"),
                    path,
                    child.span,
                )));
            }
        }
    }
    if let Some(name) = extra
        && !captures.insert(name.clone())
    {
        return Err(DiagnosticSet::one(Diagnostic::new(
            "RIM-SEMA-001",
            format!("pattern captures `{name}` more than once"),
            path,
            span,
        )));
    }
    Ok(captures)
}

fn pattern_is_irrefutable(pattern: &syntax::Pattern) -> bool {
    match &pattern.kind {
        syntax::PatternKind::Capture(_) | syntax::PatternKind::Wildcard => true,
        syntax::PatternKind::As { pattern, .. } => pattern_is_irrefutable(pattern),
        syntax::PatternKind::Or(patterns) => patterns.iter().any(pattern_is_irrefutable),
        syntax::PatternKind::Star(_) => true,
        syntax::PatternKind::Value(_)
        | syntax::PatternKind::SingletonNone
        | syntax::PatternKind::SingletonBool(_)
        | syntax::PatternKind::Sequence(_)
        | syntax::PatternKind::Mapping { .. }
        | syntax::PatternKind::Class { .. } => false,
    }
}

fn sema_error(path: &Path, message: impl Into<String>) -> DiagnosticSet {
    sema_error_at(path, Span::default(), message)
}

fn sema_error_at(path: &Path, span: Span, message: impl Into<String>) -> DiagnosticSet {
    DiagnosticSet::one(Diagnostic::new("RIM-SEMA-001", message, path, span))
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
        syntax::StatementKind::AnnAssign {
            target,
            annotation,
            value,
            ..
        } => {
            target_contains_zero_argument_super(target)
                || expression_contains_zero_argument_super(annotation)
                || value
                    .as_ref()
                    .is_some_and(expression_contains_zero_argument_super)
        }
        syntax::StatementKind::Assert { test, message } => {
            expression_contains_zero_argument_super(test)
                || message
                    .as_ref()
                    .is_some_and(expression_contains_zero_argument_super)
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
        syntax::StatementKind::Match { subject, cases } => {
            expression_contains_zero_argument_super(subject)
                || cases.iter().any(|case| {
                    pattern_contains_zero_argument_super(&case.pattern)
                        || case
                            .guard
                            .as_ref()
                            .is_some_and(expression_contains_zero_argument_super)
                        || contains_zero_argument_super(&case.body)
                })
        }
        syntax::StatementKind::TypeAlias { value, .. } => {
            expression_contains_zero_argument_super(value)
        }
        syntax::StatementKind::FunctionDef { .. }
        | syntax::StatementKind::ClassDef { .. }
        | syntax::StatementKind::Import { .. }
        | syntax::StatementKind::Return { value: None }
        | syntax::StatementKind::Break
        | syntax::StatementKind::Continue
        | syntax::StatementKind::Global(_)
        | syntax::StatementKind::Nonlocal(_) => false,
    }
}

fn pattern_contains_zero_argument_super(pattern: &syntax::Pattern) -> bool {
    match &pattern.kind {
        syntax::PatternKind::Value(value) => expression_contains_zero_argument_super(value),
        syntax::PatternKind::As { pattern, .. } => pattern_contains_zero_argument_super(pattern),
        syntax::PatternKind::Or(patterns) | syntax::PatternKind::Sequence(patterns) => {
            patterns.iter().any(pattern_contains_zero_argument_super)
        }
        syntax::PatternKind::Mapping { keys, patterns, .. } => {
            keys.iter().any(expression_contains_zero_argument_super)
                || patterns.iter().any(pattern_contains_zero_argument_super)
        }
        syntax::PatternKind::Class {
            class,
            positional,
            keywords,
        } => {
            expression_contains_zero_argument_super(class)
                || positional.iter().any(pattern_contains_zero_argument_super)
                || keywords
                    .iter()
                    .any(|(_, pattern)| pattern_contains_zero_argument_super(pattern))
        }
        syntax::PatternKind::SingletonNone
        | syntax::PatternKind::SingletonBool(_)
        | syntax::PatternKind::Capture(_)
        | syntax::PatternKind::Wildcard
        | syntax::PatternKind::Star(_) => false,
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
        | syntax::ExpressionKind::JoinedString(values)
        | syntax::ExpressionKind::Boolean { values, .. } => {
            values.iter().any(expression_contains_zero_argument_super)
        }
        syntax::ExpressionKind::Dictionary(values) => values.iter().any(|entry| match entry {
            syntax::DictionaryEntry::Pair { key, value } => {
                expression_contains_zero_argument_super(key)
                    || expression_contains_zero_argument_super(value)
            }
            syntax::DictionaryEntry::Unpack(value) => {
                expression_contains_zero_argument_super(value)
            }
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
        syntax::ExpressionKind::Name(name) => name == "__class__",
        syntax::ExpressionKind::Lambda { .. }
        | syntax::ExpressionKind::None
        | syntax::ExpressionKind::Bool(_)
        | syntax::ExpressionKind::Int(_)
        | syntax::ExpressionKind::Float(_)
        | syntax::ExpressionKind::String(_)
        | syntax::ExpressionKind::Bytes(_)
        | syntax::ExpressionKind::Complex { .. } => false,
        syntax::ExpressionKind::Binary { left, right, .. } => {
            expression_contains_zero_argument_super(left)
                || expression_contains_zero_argument_super(right)
        }
        syntax::ExpressionKind::NamedExpression { value, .. } => {
            expression_contains_zero_argument_super(value)
        }
        syntax::ExpressionKind::Yield { value } => value
            .as_deref()
            .is_some_and(expression_contains_zero_argument_super),
        syntax::ExpressionKind::YieldFrom { value } => {
            expression_contains_zero_argument_super(value)
        }
        syntax::ExpressionKind::Comprehension {
            element,
            key,
            clauses,
            ..
        } => {
            expression_contains_zero_argument_super(element)
                || key
                    .as_deref()
                    .is_some_and(expression_contains_zero_argument_super)
                || clauses.iter().any(|clause| {
                    target_contains_zero_argument_super(&clause.target)
                        || expression_contains_zero_argument_super(&clause.iterable)
                        || clause
                            .filters
                            .iter()
                            .any(expression_contains_zero_argument_super)
                })
        }
        syntax::ExpressionKind::FormattedValue {
            value, format_spec, ..
        } => {
            expression_contains_zero_argument_super(value)
                || format_spec
                    .as_ref()
                    .is_some_and(|spec| expression_contains_zero_argument_super(spec))
        }
        syntax::ExpressionKind::Compare { left, comparisons } => {
            expression_contains_zero_argument_super(left)
                || comparisons
                    .iter()
                    .any(|(_, right)| expression_contains_zero_argument_super(right))
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
        let dynamic_builtin_stable =
            ["eval", "exec", "compile"].map(|name| !raw.mutates_global_name(name, true));
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
            dynamic_builtin_stable,
            class_depth: 0,
            definition_type_params: BTreeSet::new(),
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
        assert_eq!(diagnostic.code, "RIM-SEMA-001");
        assert!(diagnostic.span.end > diagnostic.span.start);
    }

    #[test]
    fn comprehension_scope_keeps_targets_local_and_walrus_targets_in_the_owner_scope() {
        let source = r#"
def f():
    outer = 10
    captured = -1
    return [outer + (captured := item) for item in [1, 2]]
"#;
        let module = syntax::parse(path(), source).unwrap();
        let hir = analyze(path(), &module).unwrap();
        let hir::StatementKind::FunctionDef { body, cells, .. } = &hir.statements[0].kind else {
            panic!("expected function definition");
        };
        assert!(cells.iter().any(|name| name == "outer"));
        assert!(cells.iter().any(|name| name == "captured"));
        let hir::StatementKind::Return { value: Some(value) } = &body[2].kind else {
            panic!("expected comprehension return");
        };
        let hir::ExpressionKind::Comprehension {
            locals,
            free,
            clauses,
            ..
        } = &value.kind
        else {
            panic!("expected comprehension HIR");
        };
        assert!(locals.iter().any(|name| name == "item"));
        assert!(
            !locals
                .iter()
                .any(|name| name == "outer" || name == "captured")
        );
        assert!(free.iter().any(|name| name == "outer"));
        assert!(free.iter().any(|name| name == "captured"));
        assert!(matches!(
            clauses[0].target.kind,
            hir::TargetKind::Name {
                ref name,
                binding: hir::Binding::Local
            } if name == "item"
        ));
    }

    #[test]
    fn match_capture_sets_reject_duplicates_inconsistent_or_and_unreachable_alternatives() {
        for (source, expected_fragment) in [
            (
                "match 1:\n    case (1 as value) as value:\n        result = value\n",
                "more than once",
            ),
            (
                "match 1:\n    case (1 as left) | (2 as right):\n        result = 1\n",
                "must bind the same names",
            ),
            (
                "match 1:\n    case captured | 1:\n        result = captured\n",
                "makes later alternatives unreachable",
            ),
        ] {
            let module = syntax::parse(path(), source).unwrap_or_else(|diagnostics| {
                panic!("source should parse: {source}: {diagnostics:?}")
            });
            let diagnostics = analyze(path(), &module).unwrap_err();
            let diagnostic = &diagnostics.as_slice()[0];
            assert_eq!(diagnostic.code, "RIM-SEMA-001", "source: {source}");
            assert!(
                diagnostic.span.end > diagnostic.span.start,
                "{diagnostic:?}"
            );
            assert!(
                diagnostic.message.contains(expected_fragment),
                "source: {source}: {diagnostic:?}"
            );
        }
    }

    #[test]
    fn comprehension_walrus_restrictions_and_generator_scope_are_explicit() {
        for (source, expected_fragment) in [
            (
                "[value for value in (owner := [1, 2])]\n",
                "cannot be used in a comprehension iterable expression",
            ),
            (
                "[(value := 3) for value in [1, 2]]\n",
                "cannot rebind comprehension iteration variable",
            ),
            (
                "class C:\n    [(owner := value) for value in [1, 2]]\n",
                "cannot be used in a class body",
            ),
        ] {
            let module = syntax::parse(path(), source).unwrap();
            let diagnostics = analyze(path(), &module).unwrap_err();
            let diagnostic = &diagnostics.as_slice()[0];
            assert_eq!(diagnostic.code, "RIM-SEMA-001", "source: {source}");
            assert!(
                diagnostic.message.contains(expected_fragment),
                "{diagnostic:?}"
            );
        }

        let module = syntax::parse(path(), "(value for value in [1, 2])\n").unwrap();
        let hir = analyze(path(), &module).unwrap();
        let hir::StatementKind::Expression(hir::Expression {
            kind:
                hir::ExpressionKind::Comprehension {
                    kind: hir::ComprehensionKind::Generator,
                    locals,
                    clauses,
                    ..
                },
            ..
        }) = &hir.statements[0].kind
        else {
            panic!("expected generator-expression HIR");
        };
        assert_eq!(locals, &["value".to_owned()]);
        assert!(matches!(
            clauses[0].target.kind,
            hir::TargetKind::Name {
                ref name,
                binding: hir::Binding::Local
            } if name == "value"
        ));
    }

    #[test]
    fn gate5_function_analysis_preserves_decorators_defaults_annotations_and_scope_ownership() {
        let source = r#"
marker = 7

def decorate(function):
    return function

@decorate
def outer(value: int = marker, *, flag: int = marker) -> int:
    captured = value
    def inner():
        return captured
    return inner
"#;
        let module = syntax::parse(path(), source).unwrap();
        let hir = analyze(path(), &module).unwrap();
        let hir::StatementKind::FunctionDef {
            decorators,
            parameters,
            return_annotation,
            body,
            cells,
            ..
        } = &hir.statements[2].kind
        else {
            panic!("expected outer function");
        };
        assert_eq!(decorators.len(), 1);
        assert_eq!(parameters.len(), 2);
        assert!(
            parameters
                .iter()
                .all(|parameter| parameter.default.is_some())
        );
        assert!(
            parameters
                .iter()
                .all(|parameter| parameter.annotation.is_some())
        );
        assert!(return_annotation.is_some());
        assert!(cells.iter().any(|name| name == "captured"));
        let hir::StatementKind::FunctionDef { free, .. } = &body[1].kind else {
            panic!("expected inner function");
        };
        assert_eq!(free, &["captured".to_owned()]);
    }

    #[test]
    fn gate7_local_observation_order_preserves_parameters_then_first_bindings() {
        let source = r#"
def ordered(z, a):
    q = 1
    b = 2
    q = 3
    for item in [1]:
        loop_value = item
    return locals()
"#;
        let module = syntax::parse(path(), source).unwrap();
        let hir = analyze(path(), &module).unwrap();
        let hir::StatementKind::FunctionDef { locals, .. } = &hir.statements[0].kind else {
            panic!("expected ordered function");
        };
        assert_eq!(
            locals,
            &[
                "z".to_owned(),
                "a".to_owned(),
                "q".to_owned(),
                "b".to_owned(),
                "item".to_owned(),
                "loop_value".to_owned(),
            ]
        );
    }
}
