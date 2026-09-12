use std::path::Path;

use crate::core::{Diagnostic, DiagnosticSet, Span};
pub use rimera_abi::{
    RBinaryOperator as BinaryOperator, RCompareOperator as CompareOperator,
    RDynamicCompileMode as CompileMode, RUnaryOperator as UnaryOperator,
};
use rustpython_parser::ast::Ranged;
use rustpython_parser::{Parse, ast};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanOperator {
    And,
    Or,
}

#[derive(Debug, Clone)]
pub struct Module {
    pub filename: String,
    pub line_starts: Vec<u32>,
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub struct Statement {
    pub span: Span,
    pub kind: StatementKind,
}

#[derive(Debug, Clone)]
pub enum StatementKind {
    Assign {
        targets: Vec<Target>,
        value: Expression,
    },
    AugAssign {
        target: Target,
        op: BinaryOperator,
        value: Expression,
    },
    Delete {
        targets: Vec<Target>,
    },
    AnnAssign {
        target: Target,
        annotation: Expression,
        value: Option<Expression>,
        simple: bool,
    },
    Assert {
        test: Expression,
        message: Option<Expression>,
    },
    FunctionDef {
        name: String,
        is_async: bool,
        type_params: Vec<TypeParameter>,
        decorators: Vec<Expression>,
        parameters: Vec<Parameter>,
        return_annotation: Option<Expression>,
        body: Vec<Statement>,
    },
    ClassDef {
        name: String,
        type_params: Vec<TypeParameter>,
        decorators: Vec<Expression>,
        bases: Vec<Expression>,
        metaclass: Option<Expression>,
        keywords: Vec<(String, Expression)>,
        body: Vec<Statement>,
    },
    TypeAlias {
        name: String,
        type_params: Vec<TypeParameter>,
        value: Expression,
    },
    Return {
        value: Option<Expression>,
    },
    Import {
        aliases: Vec<ImportAlias>,
    },
    ImportFrom {
        module: Option<String>,
        level: u32,
        aliases: Vec<ImportAlias>,
    },
    Break,
    Continue,
    Expression(Expression),
    Global(Vec<String>),
    Nonlocal(Vec<String>),
    Raise {
        exception: Option<Expression>,
        cause: Option<Expression>,
    },
    Try {
        body: Vec<Statement>,
        handlers: Vec<ExceptionHandler>,
        else_body: Vec<Statement>,
        finally_body: Vec<Statement>,
        is_star: bool,
    },
    With {
        items: Vec<WithItem>,
        body: Vec<Statement>,
        is_async: bool,
    },
    Print {
        values: Vec<Expression>,
    },
    If {
        condition: Expression,
        then_body: Vec<Statement>,
        else_body: Vec<Statement>,
    },
    While {
        condition: Expression,
        body: Vec<Statement>,
    },
    For {
        target: Target,
        iterable: Expression,
        body: Vec<Statement>,
        else_body: Vec<Statement>,
        is_async: bool,
    },
    Match {
        subject: Expression,
        cases: Vec<MatchCase>,
    },
}

#[derive(Debug, Clone)]
pub struct WithItem {
    pub context: Expression,
    pub target: Option<Target>,
}

#[derive(Debug, Clone)]
pub struct ImportAlias {
    pub module: String,
    pub bind_name: String,
    pub explicit_alias: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeParameterKind {
    TypeVar,
    TypeVarTuple,
    ParamSpec,
}

#[derive(Debug, Clone)]
pub struct TypeParameter {
    pub span: Span,
    pub name: String,
    pub kind: TypeParameterKind,
}

#[derive(Debug, Clone)]
pub struct MatchCase {
    pub pattern: Pattern,
    pub guard: Option<Expression>,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub struct Pattern {
    pub span: Span,
    pub kind: PatternKind,
}

#[derive(Debug, Clone)]
pub enum PatternKind {
    Value(Expression),
    SingletonNone,
    SingletonBool(bool),
    Capture(String),
    Wildcard,
    As {
        pattern: Box<Pattern>,
        name: String,
    },
    Or(Vec<Pattern>),
    Sequence(Vec<Pattern>),
    Star(Option<String>),
    Mapping {
        keys: Vec<Expression>,
        patterns: Vec<Pattern>,
        rest: Option<String>,
    },
    Class {
        class: Expression,
        positional: Vec<Pattern>,
        keywords: Vec<(String, Pattern)>,
    },
}

#[derive(Debug, Clone)]
pub struct Target {
    pub span: Span,
    pub kind: TargetKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceTargetKind {
    Tuple,
    List,
}

#[derive(Debug, Clone)]
pub enum TargetKind {
    Name(String),
    Attribute {
        receiver: Expression,
        name: String,
    },
    Item {
        collection: Expression,
        index: Expression,
    },
    Sequence {
        kind: SequenceTargetKind,
        elements: Vec<Target>,
    },
    Starred(Box<Target>),
}

#[derive(Debug, Clone)]
pub struct ExceptionHandler {
    pub exception_type: Option<Expression>,
    pub name: Option<String>,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub kind: ParameterKind,
    pub default: Option<Expression>,
    pub annotation: Option<Expression>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterKind {
    PositionalOnly,
    PositionalOrKeyword,
    VarArgs,
    KeywordOnly,
    VarKeywords,
}

#[derive(Debug, Clone)]
pub struct Expression {
    pub span: Span,
    pub kind: ExpressionKind,
}

#[derive(Debug, Clone)]
pub enum DictionaryEntry {
    Pair { key: Expression, value: Expression },
    Unpack(Expression),
}

#[derive(Debug, Clone)]
pub enum CallPart {
    Positional(Expression),
    Starred(Expression),
    Keyword { name: String, value: Expression },
    KeywordUnpack(Expression),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatConversion {
    None,
    Str,
    Repr,
    Ascii,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComprehensionKind {
    List,
    Set,
    Dictionary,
    Generator,
}

#[derive(Debug, Clone)]
pub struct ComprehensionClause {
    pub target: Target,
    pub iterable: Expression,
    pub filters: Vec<Expression>,
    pub is_async: bool,
}

#[derive(Debug, Clone)]
pub enum ExpressionKind {
    None,
    Bool(bool),
    Int(String),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Complex {
        real: f64,
        imag: f64,
    },
    Slice {
        start: Option<Box<Expression>>,
        stop: Option<Box<Expression>>,
        step: Option<Box<Expression>>,
    },
    List(Vec<Expression>),
    Tuple(Vec<Expression>),
    Dictionary(Vec<DictionaryEntry>),
    Set(Vec<Expression>),
    Subscript {
        value: Box<Expression>,
        index: Box<Expression>,
    },
    Attribute {
        value: Box<Expression>,
        name: String,
    },
    Lambda {
        parameters: Vec<Parameter>,
        body: Box<Expression>,
    },
    Name(String),
    Unary {
        op: UnaryOperator,
        operand: Box<Expression>,
    },
    Boolean {
        op: BooleanOperator,
        values: Vec<Expression>,
    },
    Binary {
        op: BinaryOperator,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Compare {
        left: Box<Expression>,
        comparisons: Vec<(CompareOperator, Expression)>,
    },
    NamedExpression {
        name: String,
        value: Box<Expression>,
    },
    Yield {
        value: Option<Box<Expression>>,
    },
    YieldFrom {
        value: Box<Expression>,
    },
    Await {
        value: Box<Expression>,
    },
    Comprehension {
        kind: ComprehensionKind,
        element: Box<Expression>,
        key: Option<Box<Expression>>,
        clauses: Vec<ComprehensionClause>,
    },
    JoinedString(Vec<Expression>),
    FormattedValue {
        value: Box<Expression>,
        conversion: FormatConversion,
        format_spec: Option<Box<Expression>>,
    },
    Call {
        callable: Box<Expression>,
        parts: Vec<CallPart>,
    },
}

pub fn parse(path: &Path, source: &str) -> Result<Module, DiagnosticSet> {
    parse_mode(path, source, CompileMode::Exec)
}

/// Parses one of Python's three `compile()` grammar modes through the same
/// RustPython-parser front end used by ordinary Rimera source builds. No AST
/// interpreter or alternate dynamic grammar exists behind this entry point.
pub fn parse_mode(
    path: &Path,
    source: &str,
    mode: CompileMode,
) -> Result<Module, DiagnosticSet> {
    let source_path = path.to_string_lossy();
    let parse_error = |error: rustpython_parser::ParseError| {
        DiagnosticSet::one(Diagnostic::new(
            "RIM-PARSE-001",
            error.to_string(),
            path,
            Span::new(error.offset.to_u32(), error.offset.to_u32()),
        ))
    };
    let statements = match mode {
        CompileMode::Exec => ast::Suite::parse(source, &source_path)
            .map_err(parse_error)?
            .iter()
            .map(|statement| convert_statement(path, statement))
            .collect::<Result<Vec<_>, _>>()?,
        CompileMode::Eval => {
            let expression = ast::ModExpression::parse(source, &source_path).map_err(parse_error)?;
            let expression = convert_expression(path, &expression.body)?;
            vec![Statement {
                span: expression.span,
                kind: StatementKind::Expression(expression),
            }]
        }
        CompileMode::Single => {
            let interactive = ast::ModInteractive::parse(source, &source_path).map_err(parse_error)?;
            if interactive.body.windows(2).any(|pair| {
                source[pair[0].end().to_usize()..pair[1].start().to_usize()].contains('\n')
            }) {
                return Err(DiagnosticSet::one(Diagnostic::new(
                    "RIM-PARSE-001",
                    "multiple statements found while compiling a single statement",
                    path,
                    Span::default(),
                )));
            }
            interactive
                .body
                .iter()
                .map(|statement| convert_statement(path, statement))
                .collect::<Result<Vec<_>, _>>()?
        }
    };
    let line_starts = std::iter::once(0)
        .chain(
            source
                .bytes()
                .enumerate()
                .filter_map(|(index, byte)| (byte == b'\n').then_some(index as u32 + 1)),
        )
        .collect();
    Ok(Module {
        filename: path.to_string_lossy().into_owned(),
        line_starts,
        statements,
    })
}

fn convert_statement(path: &Path, statement: &ast::Stmt) -> Result<Statement, DiagnosticSet> {
    use ast::Ranged;
    let span = span_of(statement.range());
    let kind = match statement {
        ast::Stmt::FunctionDef(node) => StatementKind::FunctionDef {
            name: node.name.to_string(),
            is_async: false,
            type_params: convert_type_parameters(path, &node.type_params)?,
            decorators: node
                .decorator_list
                .iter()
                .map(|decorator| convert_expression(path, decorator))
                .collect::<Result<Vec<_>, _>>()?,
            parameters: convert_parameters(path, &node.args)?,
            return_annotation: node
                .returns
                .as_deref()
                .map(|annotation| convert_expression(path, annotation))
                .transpose()?,
            body: convert_statements(path, &node.body)?,
        },
        ast::Stmt::AsyncFunctionDef(node) => StatementKind::FunctionDef {
            name: node.name.to_string(),
            is_async: true,
            type_params: convert_type_parameters(path, &node.type_params)?,
            decorators: node
                .decorator_list
                .iter()
                .map(|decorator| convert_expression(path, decorator))
                .collect::<Result<Vec<_>, _>>()?,
            parameters: convert_parameters(path, &node.args)?,
            return_annotation: node
                .returns
                .as_deref()
                .map(|annotation| convert_expression(path, annotation))
                .transpose()?,
            body: convert_statements(path, &node.body)?,
        },
        ast::Stmt::ClassDef(node) => {
            let type_params = convert_type_parameters(path, &node.type_params)?;
            let mut metaclass = None;
            let mut keywords = Vec::new();
            for keyword in &node.keywords {
                let Some(name) = &keyword.arg else {
                    return unsupported(
                        path,
                        span_of(keyword.value.range()),
                        "class keyword unpacking requires the complete class protocol",
                    );
                };
                if name.as_str() == "metaclass" && metaclass.is_none() {
                    metaclass = Some(convert_expression(path, &keyword.value)?);
                    continue;
                }
                if name.as_str() == "metaclass" {
                    return unsupported(
                        path,
                        span_of(keyword.value.range()),
                        "duplicate `metaclass` class keyword",
                    );
                }
                keywords.push((name.to_string(), convert_expression(path, &keyword.value)?));
            }
            let mut bases = Vec::with_capacity(node.bases.len());
            for base in &node.bases {
                let ast::Expr::Name(base_name) = base else {
                    return unsupported(
                        path,
                        span_of(base.range()),
                        "class bases must name an existing native user class",
                    );
                };
                if matches!(base_name.id.as_str(), "bool" | "int") {
                    return unsupported(
                        path,
                        span_of(base.range()),
                        "subclassing builtin storage types requires native layout inheritance",
                    );
                }
                bases.push(convert_expression(path, base)?);
            }
            StatementKind::ClassDef {
                name: node.name.to_string(),
                type_params,
                decorators: node
                    .decorator_list
                    .iter()
                    .map(|decorator| convert_expression(path, decorator))
                    .collect::<Result<Vec<_>, _>>()?,
                bases,
                metaclass,
                keywords,
                body: convert_class_body(path, &node.body)?,
            }
        }
        ast::Stmt::TypeAlias(node) => {
            let ast::Expr::Name(name) = node.name.as_ref() else {
                return capability(
                    path,
                    span,
                    "RIM-CAP-G7-04",
                    "PEP 695 type aliases require a simple name target",
                );
            };
            StatementKind::TypeAlias {
                name: name.id.to_string(),
                type_params: convert_type_parameters(path, &node.type_params)?,
                value: convert_expression(path, &node.value)?,
            }
        }
        ast::Stmt::AugAssign(node) => {
            let op = match node.op {
                ast::Operator::Add => BinaryOperator::Add,
                ast::Operator::Sub => BinaryOperator::Subtract,
                ast::Operator::Mult => BinaryOperator::Multiply,
                ast::Operator::FloorDiv => BinaryOperator::FloorDivide,
                ast::Operator::Mod => BinaryOperator::Modulo,
                ast::Operator::Div => BinaryOperator::TrueDivide,
                ast::Operator::Pow => BinaryOperator::Power,
                ast::Operator::LShift => BinaryOperator::LeftShift,
                ast::Operator::RShift => BinaryOperator::RightShift,
                ast::Operator::BitAnd => BinaryOperator::BitAnd,
                ast::Operator::BitXor => BinaryOperator::BitXor,
                ast::Operator::BitOr => BinaryOperator::BitOr,
                ast::Operator::MatMult => BinaryOperator::MatrixMultiply,
            };
            StatementKind::AugAssign {
                target: convert_target(path, node.target.as_ref())?,
                op,
                value: convert_expression(path, &node.value)?,
            }
        }
        ast::Stmt::Return(node) => StatementKind::Return {
            value: node
                .value
                .as_deref()
                .map(|value| convert_expression(path, value))
                .transpose()?,
        },
        ast::Stmt::Import(node) => {
            let mut aliases = Vec::with_capacity(node.names.len());
            for alias in &node.names {
                let module = alias.name.to_string();
                let bind_name = alias.asname.as_ref().map_or_else(
                    || module.split('.').next().unwrap_or(&module).to_owned(),
                    ToString::to_string,
                );
                aliases.push(ImportAlias {
                    module,
                    bind_name,
                    explicit_alias: alias.asname.is_some(),
                });
            }
            StatementKind::Import { aliases }
        }
        ast::Stmt::ImportFrom(node) => StatementKind::ImportFrom {
            module: node.module.as_ref().map(ToString::to_string),
            level: node.level.map_or(0, |level| level.to_u32()),
            aliases: node
                .names
                .iter()
                .map(|alias| {
                    let module = alias.name.to_string();
                    let bind_name = alias
                        .asname
                        .as_ref()
                        .map_or_else(|| module.clone(), ToString::to_string);
                    ImportAlias {
                        module,
                        bind_name,
                        explicit_alias: alias.asname.is_some(),
                    }
                })
                .collect(),
        },
        ast::Stmt::Break(_) => StatementKind::Break,
        ast::Stmt::Continue(_) => StatementKind::Continue,
        ast::Stmt::Global(node) => {
            StatementKind::Global(node.names.iter().map(ToString::to_string).collect())
        }
        ast::Stmt::Nonlocal(node) => {
            StatementKind::Nonlocal(node.names.iter().map(ToString::to_string).collect())
        }
        ast::Stmt::Raise(node) => StatementKind::Raise {
            exception: node
                .exc
                .as_deref()
                .map(|exception| convert_expression(path, exception))
                .transpose()?,
            cause: node
                .cause
                .as_deref()
                .map(|cause| convert_expression(path, cause))
                .transpose()?,
        },
        ast::Stmt::Try(node) => StatementKind::Try {
            body: convert_statements(path, &node.body)?,
            handlers: convert_handlers(path, &node.handlers)?,
            else_body: convert_statements(path, &node.orelse)?,
            finally_body: convert_statements(path, &node.finalbody)?,
            is_star: false,
        },
        ast::Stmt::TryStar(node) => StatementKind::Try {
            body: convert_statements(path, &node.body)?,
            handlers: convert_handlers(path, &node.handlers)?,
            else_body: convert_statements(path, &node.orelse)?,
            finally_body: convert_statements(path, &node.finalbody)?,
            is_star: true,
        },
        ast::Stmt::With(node) => StatementKind::With {
            items: node
                .items
                .iter()
                .map(|item| {
                    Ok(WithItem {
                        context: convert_expression(path, &item.context_expr)?,
                        target: item
                            .optional_vars
                            .as_deref()
                            .map(|target| convert_target(path, target))
                            .transpose()?,
                    })
                })
                .collect::<Result<Vec<_>, DiagnosticSet>>()?,
            body: convert_statements(path, &node.body)?,
            is_async: false,
        },
        ast::Stmt::AsyncWith(node) => StatementKind::With {
            items: node
                .items
                .iter()
                .map(|item| {
                    Ok(WithItem {
                        context: convert_expression(path, &item.context_expr)?,
                        target: item
                            .optional_vars
                            .as_deref()
                            .map(|target| convert_target(path, target))
                            .transpose()?,
                    })
                })
                .collect::<Result<Vec<_>, DiagnosticSet>>()?,
            body: convert_statements(path, &node.body)?,
            is_async: true,
        },
        ast::Stmt::Assign(node) => StatementKind::Assign {
            targets: node
                .targets
                .iter()
                .map(|target| convert_target(path, target))
                .collect::<Result<Vec<_>, _>>()?,
            value: convert_expression(path, &node.value)?,
        },
        ast::Stmt::Expr(node) => StatementKind::Expression(convert_expression(path, &node.value)?),
        ast::Stmt::AnnAssign(node) => StatementKind::AnnAssign {
            target: convert_target(path, node.target.as_ref())?,
            annotation: convert_expression(path, &node.annotation)?,
            value: node
                .value
                .as_deref()
                .map(|value| convert_expression(path, value))
                .transpose()?,
            simple: node.simple,
        },
        ast::Stmt::Assert(node) => StatementKind::Assert {
            test: convert_expression(path, &node.test)?,
            message: node
                .msg
                .as_deref()
                .map(|message| convert_expression(path, message))
                .transpose()?,
        },
        ast::Stmt::Match(node) => StatementKind::Match {
            subject: convert_expression(path, &node.subject)?,
            cases: node
                .cases
                .iter()
                .map(|case| {
                    Ok(MatchCase {
                        pattern: convert_pattern(path, &case.pattern)?,
                        guard: case
                            .guard
                            .as_deref()
                            .map(|guard| convert_expression(path, guard))
                            .transpose()?,
                        body: convert_statements(path, &case.body)?,
                    })
                })
                .collect::<Result<Vec<_>, DiagnosticSet>>()?,
        },
        ast::Stmt::Delete(node) => StatementKind::Delete {
            targets: node
                .targets
                .iter()
                .map(|target| convert_target(path, target))
                .collect::<Result<Vec<_>, _>>()?,
        },
        ast::Stmt::If(node) => StatementKind::If {
            condition: convert_expression(path, &node.test)?,
            then_body: convert_statements(path, &node.body)?,
            else_body: convert_statements(path, &node.orelse)?,
        },
        ast::Stmt::While(node) => {
            if !node.orelse.is_empty() {
                return unsupported(path, span, "`while` with an `else` suite is not supported");
            }
            StatementKind::While {
                condition: convert_expression(path, &node.test)?,
                body: convert_statements(path, &node.body)?,
            }
        }
        ast::Stmt::For(node) => StatementKind::For {
            target: convert_target(path, node.target.as_ref())?,
            iterable: convert_expression(path, &node.iter)?,
            body: convert_statements(path, &node.body)?,
            else_body: convert_statements(path, &node.orelse)?,
            is_async: false,
        },
        ast::Stmt::AsyncFor(node) => StatementKind::For {
            target: convert_target(path, node.target.as_ref())?,
            iterable: convert_expression(path, &node.iter)?,
            body: convert_statements(path, &node.body)?,
            else_body: convert_statements(path, &node.orelse)?,
            is_async: true,
        },
        _ => {
            return unsupported(
                path,
                span,
                "statement is outside the supported native subset",
            );
        }
    };
    Ok(Statement { span, kind })
}

fn convert_type_parameters(
    path: &Path,
    parameters: &[ast::TypeParam],
) -> Result<Vec<TypeParameter>, DiagnosticSet> {
    parameters
        .iter()
        .map(|parameter| {
            let (span, name, kind, bound) = match parameter {
                ast::TypeParam::TypeVar(parameter) => (
                    span_of(parameter.range),
                    parameter.name.to_string(),
                    TypeParameterKind::TypeVar,
                    parameter.bound.as_deref(),
                ),
                ast::TypeParam::TypeVarTuple(parameter) => (
                    span_of(parameter.range),
                    parameter.name.to_string(),
                    TypeParameterKind::TypeVarTuple,
                    None,
                ),
                ast::TypeParam::ParamSpec(parameter) => (
                    span_of(parameter.range),
                    parameter.name.to_string(),
                    TypeParameterKind::ParamSpec,
                    None,
                ),
            };
            if bound.is_some() {
                return capability(
                    path,
                    span,
                    "RIM-CAP-G7-03",
                    "lazy PEP 695 type-parameter bounds and constraints are deferred to the later generic-bound evaluator",
                );
            }
            Ok(TypeParameter { span, name, kind })
        })
        .collect()
}

fn convert_target(path: &Path, target: &ast::Expr) -> Result<Target, DiagnosticSet> {
    use ast::Ranged;
    let span = span_of(target.range());
    let kind = match target {
        ast::Expr::Name(name) => TargetKind::Name(name.id.to_string()),
        ast::Expr::Attribute(target) => TargetKind::Attribute {
            receiver: convert_expression(path, &target.value)?,
            name: target.attr.to_string(),
        },
        ast::Expr::Subscript(target) => TargetKind::Item {
            collection: convert_expression(path, &target.value)?,
            index: convert_expression(path, &target.slice)?,
        },
        ast::Expr::Tuple(tuple) => TargetKind::Sequence {
            kind: SequenceTargetKind::Tuple,
            elements: tuple
                .elts
                .iter()
                .map(|element| convert_target(path, element))
                .collect::<Result<Vec<_>, _>>()?,
        },
        ast::Expr::List(list) => TargetKind::Sequence {
            kind: SequenceTargetKind::List,
            elements: list
                .elts
                .iter()
                .map(|element| convert_target(path, element))
                .collect::<Result<Vec<_>, _>>()?,
        },
        ast::Expr::Starred(starred) => {
            TargetKind::Starred(Box::new(convert_target(path, &starred.value)?))
        }
        _ => {
            return capability(
                path,
                span,
                "RIM-SEMA-001",
                "invalid assignment target shape",
            );
        }
    };
    Ok(Target { span, kind })
}

fn convert_handlers(
    path: &Path,
    handlers: &[ast::ExceptHandler],
) -> Result<Vec<ExceptionHandler>, DiagnosticSet> {
    handlers
        .iter()
        .map(|handler| {
            let ast::ExceptHandler::ExceptHandler(handler) = handler;
            Ok(ExceptionHandler {
                exception_type: handler
                    .type_
                    .as_deref()
                    .map(|exception_type| convert_expression(path, exception_type))
                    .transpose()?,
                name: handler.name.as_ref().map(ToString::to_string),
                body: convert_statements(path, &handler.body)?,
            })
        })
        .collect()
}

fn convert_parameters(
    path: &Path,
    arguments: &ast::Arguments,
) -> Result<Vec<Parameter>, DiagnosticSet> {
    fn convert(
        path: &Path,
        argument: &ast::ArgWithDefault,
        kind: ParameterKind,
    ) -> Result<Parameter, DiagnosticSet> {
        Ok(Parameter {
            name: argument.def.arg.to_string(),
            kind,
            default: argument
                .default
                .as_deref()
                .map(|default| convert_expression(path, default))
                .transpose()?,
            annotation: argument
                .def
                .annotation
                .as_deref()
                .map(|annotation| convert_expression(path, annotation))
                .transpose()?,
        })
    }
    let mut parameters = Vec::new();
    for argument in &arguments.posonlyargs {
        parameters.push(convert(path, argument, ParameterKind::PositionalOnly)?);
    }
    for argument in &arguments.args {
        parameters.push(convert(path, argument, ParameterKind::PositionalOrKeyword)?);
    }
    if let Some(argument) = &arguments.vararg {
        parameters.push(Parameter {
            name: argument.arg.to_string(),
            kind: ParameterKind::VarArgs,
            default: None,
            annotation: argument
                .annotation
                .as_deref()
                .map(|annotation| convert_expression(path, annotation))
                .transpose()?,
        });
    }
    for argument in &arguments.kwonlyargs {
        parameters.push(convert(path, argument, ParameterKind::KeywordOnly)?);
    }
    if let Some(argument) = &arguments.kwarg {
        parameters.push(Parameter {
            name: argument.arg.to_string(),
            kind: ParameterKind::VarKeywords,
            default: None,
            annotation: argument
                .annotation
                .as_deref()
                .map(|annotation| convert_expression(path, annotation))
                .transpose()?,
        });
    }
    Ok(parameters)
}

fn convert_statements(
    path: &Path,
    statements: &[ast::Stmt],
) -> Result<Vec<Statement>, DiagnosticSet> {
    statements
        .iter()
        .map(|statement| convert_statement(path, statement))
        .collect()
}

fn convert_class_body(
    path: &Path,
    statements: &[ast::Stmt],
) -> Result<Vec<Statement>, DiagnosticSet> {
    let mut body = Vec::new();
    for statement in statements {
        if matches!(statement, ast::Stmt::Pass(_)) {
            continue;
        }
        let converted = convert_statement(path, statement)?;
        if !matches!(
            converted.kind,
            StatementKind::Assign { .. }
                | StatementKind::FunctionDef { .. }
                | StatementKind::If { .. }
                | StatementKind::While { .. }
                | StatementKind::Expression(_)
                | StatementKind::Print { .. }
                | StatementKind::AugAssign { .. }
                | StatementKind::For { .. }
                | StatementKind::Delete { .. }
                | StatementKind::AnnAssign { .. }
                | StatementKind::Assert { .. }
                | StatementKind::Raise { .. }
                | StatementKind::Try { .. }
                | StatementKind::With { .. }
                | StatementKind::ClassDef { .. }
                | StatementKind::Import { .. }
                | StatementKind::Global(_)
                | StatementKind::Nonlocal(_)
                | StatementKind::Match { .. }
        ) {
            return unsupported(
                path,
                converted.span,
                "class body statement is outside the supported native subset",
            );
        }
        body.push(converted);
    }
    Ok(body)
}

fn convert_comprehension_clauses(
    path: &Path,
    generators: &[ast::Comprehension],
) -> Result<Vec<ComprehensionClause>, DiagnosticSet> {
    generators
        .iter()
        .map(|generator| {
            Ok(ComprehensionClause {
                target: convert_target(path, &generator.target)?,
                iterable: convert_expression(path, &generator.iter)?,
                filters: generator
                    .ifs
                    .iter()
                    .map(|filter| convert_expression(path, filter))
                    .collect::<Result<Vec<_>, _>>()?,
                is_async: generator.is_async,
            })
        })
        .collect()
}

fn convert_expression(path: &Path, expression: &ast::Expr) -> Result<Expression, DiagnosticSet> {
    use ast::Ranged;
    let span = span_of(expression.range());
    let kind = match expression {
        ast::Expr::Constant(node) => match &node.value {
            ast::Constant::None => ExpressionKind::None,
            ast::Constant::Bool(value) => ExpressionKind::Bool(*value),
            ast::Constant::Int(value) => ExpressionKind::Int(value.to_string()),
            ast::Constant::Float(value) => ExpressionKind::Float(*value),
            ast::Constant::Str(value) => ExpressionKind::String(value.clone()),
            ast::Constant::Bytes(value) => ExpressionKind::Bytes(value.clone()),
            ast::Constant::Complex { real, imag } => ExpressionKind::Complex {
                real: *real,
                imag: *imag,
            },
            _ => return unsupported(path, span, "literal is outside the supported native subset"),
        },
        ast::Expr::Name(node) => ExpressionKind::Name(node.id.to_string()),
        ast::Expr::Tuple(node) => ExpressionKind::Tuple(
            node.elts
                .iter()
                .map(|element| convert_expression(path, element))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        ast::Expr::List(node) => ExpressionKind::List(
            node.elts
                .iter()
                .map(|element| convert_expression(path, element))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        ast::Expr::Dict(node) => {
            let mut entries = Vec::with_capacity(node.keys.len());
            for (key, value) in node.keys.iter().zip(&node.values) {
                entries.push(match key {
                    Some(key) => DictionaryEntry::Pair {
                        key: convert_expression(path, key)?,
                        value: convert_expression(path, value)?,
                    },
                    None => DictionaryEntry::Unpack(convert_expression(path, value)?),
                });
            }
            ExpressionKind::Dictionary(entries)
        }
        ast::Expr::Set(node) => ExpressionKind::Set(
            node.elts
                .iter()
                .map(|value| convert_expression(path, value))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        ast::Expr::Subscript(node) => ExpressionKind::Subscript {
            value: Box::new(convert_expression(path, &node.value)?),
            index: Box::new(convert_expression(path, &node.slice)?),
        },
        ast::Expr::Slice(node) => ExpressionKind::Slice {
            start: node
                .lower
                .as_deref()
                .map(|value| convert_expression(path, value))
                .transpose()?
                .map(Box::new),
            stop: node
                .upper
                .as_deref()
                .map(|value| convert_expression(path, value))
                .transpose()?
                .map(Box::new),
            step: node
                .step
                .as_deref()
                .map(|value| convert_expression(path, value))
                .transpose()?
                .map(Box::new),
        },
        ast::Expr::Attribute(node) => ExpressionKind::Attribute {
            value: Box::new(convert_expression(path, &node.value)?),
            name: node.attr.to_string(),
        },
        ast::Expr::Lambda(node) => ExpressionKind::Lambda {
            parameters: convert_parameters(path, &node.args)?,
            body: Box::new(convert_expression(path, &node.body)?),
        },
        ast::Expr::UnaryOp(node) => {
            let op = match node.op {
                ast::UnaryOp::UAdd => UnaryOperator::Positive,
                ast::UnaryOp::USub => UnaryOperator::Negate,
                ast::UnaryOp::Invert => UnaryOperator::Invert,
                ast::UnaryOp::Not => UnaryOperator::Not,
            };
            ExpressionKind::Unary {
                op,
                operand: Box::new(convert_expression(path, &node.operand)?),
            }
        }
        ast::Expr::BoolOp(node) => ExpressionKind::Boolean {
            op: match node.op {
                ast::BoolOp::And => BooleanOperator::And,
                ast::BoolOp::Or => BooleanOperator::Or,
            },
            values: node
                .values
                .iter()
                .map(|value| convert_expression(path, value))
                .collect::<Result<Vec<_>, _>>()?,
        },
        ast::Expr::BinOp(node) => {
            let op = match node.op {
                ast::Operator::Add => BinaryOperator::Add,
                ast::Operator::Sub => BinaryOperator::Subtract,
                ast::Operator::Mult => BinaryOperator::Multiply,
                ast::Operator::FloorDiv => BinaryOperator::FloorDivide,
                ast::Operator::Mod => BinaryOperator::Modulo,
                ast::Operator::Div => BinaryOperator::TrueDivide,
                ast::Operator::Pow => BinaryOperator::Power,
                ast::Operator::LShift => BinaryOperator::LeftShift,
                ast::Operator::RShift => BinaryOperator::RightShift,
                ast::Operator::BitAnd => BinaryOperator::BitAnd,
                ast::Operator::BitXor => BinaryOperator::BitXor,
                ast::Operator::BitOr => BinaryOperator::BitOr,
                ast::Operator::MatMult => BinaryOperator::MatrixMultiply,
            };
            ExpressionKind::Binary {
                op,
                left: Box::new(convert_expression(path, &node.left)?),
                right: Box::new(convert_expression(path, &node.right)?),
            }
        }
        ast::Expr::Compare(node) => {
            let comparisons = node
                .ops
                .iter()
                .zip(&node.comparators)
                .map(|(op, comparator)| {
                    let op = match op {
                        ast::CmpOp::Eq => CompareOperator::Equal,
                        ast::CmpOp::NotEq => CompareOperator::NotEqual,
                        ast::CmpOp::Lt => CompareOperator::Less,
                        ast::CmpOp::LtE => CompareOperator::LessEqual,
                        ast::CmpOp::Gt => CompareOperator::Greater,
                        ast::CmpOp::GtE => CompareOperator::GreaterEqual,
                        ast::CmpOp::In => CompareOperator::In,
                        ast::CmpOp::NotIn => CompareOperator::NotIn,
                        ast::CmpOp::Is => CompareOperator::Is,
                        ast::CmpOp::IsNot => CompareOperator::IsNot,
                    };
                    Ok((op, convert_expression(path, comparator)?))
                })
                .collect::<Result<Vec<_>, DiagnosticSet>>()?;
            ExpressionKind::Compare {
                left: Box::new(convert_expression(path, &node.left)?),
                comparisons,
            }
        }
        ast::Expr::NamedExpr(node) => {
            let target = convert_target(path, node.target.as_ref())?;
            let TargetKind::Name(name) = target.kind else {
                return unsupported(
                    path,
                    target.span,
                    "assignment-expression targets must be names",
                );
            };
            ExpressionKind::NamedExpression {
                name,
                value: Box::new(convert_expression(path, &node.value)?),
            }
        }
        ast::Expr::Yield(node) => ExpressionKind::Yield {
            value: node
                .value
                .as_deref()
                .map(|value| convert_expression(path, value))
                .transpose()?
                .map(Box::new),
        },
        ast::Expr::YieldFrom(node) => ExpressionKind::YieldFrom {
            value: Box::new(convert_expression(path, &node.value)?),
        },
        ast::Expr::Await(node) => ExpressionKind::Await {
            value: Box::new(convert_expression(path, &node.value)?),
        },
        ast::Expr::ListComp(node) => ExpressionKind::Comprehension {
            kind: ComprehensionKind::List,
            element: Box::new(convert_expression(path, &node.elt)?),
            key: None,
            clauses: convert_comprehension_clauses(path, &node.generators)?,
        },
        ast::Expr::SetComp(node) => ExpressionKind::Comprehension {
            kind: ComprehensionKind::Set,
            element: Box::new(convert_expression(path, &node.elt)?),
            key: None,
            clauses: convert_comprehension_clauses(path, &node.generators)?,
        },
        ast::Expr::DictComp(node) => ExpressionKind::Comprehension {
            kind: ComprehensionKind::Dictionary,
            element: Box::new(convert_expression(path, &node.value)?),
            key: Some(Box::new(convert_expression(path, &node.key)?)),
            clauses: convert_comprehension_clauses(path, &node.generators)?,
        },
        ast::Expr::GeneratorExp(node) => ExpressionKind::Comprehension {
            kind: ComprehensionKind::Generator,
            element: Box::new(convert_expression(path, &node.elt)?),
            key: None,
            clauses: convert_comprehension_clauses(path, &node.generators)?,
        },
        ast::Expr::JoinedStr(node) => ExpressionKind::JoinedString(
            node.values
                .iter()
                .map(|value| convert_expression(path, value))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        ast::Expr::FormattedValue(node) => ExpressionKind::FormattedValue {
            value: Box::new(convert_expression(path, &node.value)?),
            conversion: match node.conversion {
                ast::ConversionFlag::None => FormatConversion::None,
                ast::ConversionFlag::Str => FormatConversion::Str,
                ast::ConversionFlag::Repr => FormatConversion::Repr,
                ast::ConversionFlag::Ascii => FormatConversion::Ascii,
            },
            format_spec: node
                .format_spec
                .as_deref()
                .map(|spec| convert_expression(path, spec))
                .transpose()?
                .map(Box::new),
        },
        ast::Expr::Starred(_) => {
            return capability(
                path,
                span,
                "RIM-SEMA-001",
                "starred expression is not valid in this expression context",
            );
        }
        ast::Expr::Call(node) => {
            let mut ordered = Vec::with_capacity(node.args.len() + node.keywords.len());
            for argument in &node.args {
                let (offset, part) = match argument {
                    ast::Expr::Starred(starred) => (
                        u32::from(starred.range().start()),
                        CallPart::Starred(convert_expression(path, &starred.value)?),
                    ),
                    _ => (
                        u32::from(argument.range().start()),
                        CallPart::Positional(convert_expression(path, argument)?),
                    ),
                };
                ordered.push((offset, part));
            }
            for keyword in &node.keywords {
                let offset = u32::from(keyword.value.range().start());
                let value = convert_expression(path, &keyword.value)?;
                let part = match &keyword.arg {
                    Some(name) => CallPart::Keyword {
                        name: name.to_string(),
                        value,
                    },
                    None => CallPart::KeywordUnpack(value),
                };
                ordered.push((offset, part));
            }
            ordered.sort_by_key(|(offset, _)| *offset);
            ExpressionKind::Call {
                callable: Box::new(convert_expression(path, &node.func)?),
                parts: ordered.into_iter().map(|(_, part)| part).collect(),
            }
        }
        _ => {
            return unsupported(
                path,
                span,
                "expression is outside the supported native subset",
            );
        }
    };
    Ok(Expression { span, kind })
}

fn convert_pattern(path: &Path, pattern: &ast::Pattern) -> Result<Pattern, DiagnosticSet> {
    let span = span_of(pattern.range());
    let kind = match pattern {
        ast::Pattern::MatchValue(node) => {
            PatternKind::Value(convert_expression(path, &node.value)?)
        }
        ast::Pattern::MatchSingleton(node) => match node.value {
            ast::Constant::None => PatternKind::SingletonNone,
            ast::Constant::Bool(value) => PatternKind::SingletonBool(value),
            _ => {
                return unsupported(path, span, "unsupported singleton pattern");
            }
        },
        ast::Pattern::MatchAs(node) => match (&node.pattern, &node.name) {
            (None, None) => PatternKind::Wildcard,
            (None, Some(name)) => PatternKind::Capture(name.to_string()),
            (Some(pattern), Some(name)) => PatternKind::As {
                pattern: Box::new(convert_pattern(path, pattern)?),
                name: name.to_string(),
            },
            (Some(pattern), None) => return convert_pattern(path, pattern),
        },
        ast::Pattern::MatchOr(node) => PatternKind::Or(
            node.patterns
                .iter()
                .map(|pattern| convert_pattern(path, pattern))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        ast::Pattern::MatchSequence(node) => PatternKind::Sequence(
            node.patterns
                .iter()
                .map(|pattern| convert_pattern(path, pattern))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        ast::Pattern::MatchStar(node) => {
            PatternKind::Star(node.name.as_ref().map(ToString::to_string))
        }
        ast::Pattern::MatchMapping(node) => PatternKind::Mapping {
            keys: node
                .keys
                .iter()
                .map(|key| convert_expression(path, key))
                .collect::<Result<Vec<_>, _>>()?,
            patterns: node
                .patterns
                .iter()
                .map(|pattern| convert_pattern(path, pattern))
                .collect::<Result<Vec<_>, _>>()?,
            rest: node.rest.as_ref().map(ToString::to_string),
        },
        ast::Pattern::MatchClass(node) => {
            if node.kwd_attrs.len() != node.kwd_patterns.len() {
                return unsupported(path, span, "class-pattern keyword arity is invalid");
            }
            PatternKind::Class {
                class: convert_expression(path, &node.cls)?,
                positional: node
                    .patterns
                    .iter()
                    .map(|pattern| convert_pattern(path, pattern))
                    .collect::<Result<Vec<_>, _>>()?,
                keywords: node
                    .kwd_attrs
                    .iter()
                    .zip(&node.kwd_patterns)
                    .map(|(name, pattern)| Ok((name.to_string(), convert_pattern(path, pattern)?)))
                    .collect::<Result<Vec<_>, DiagnosticSet>>()?,
            }
        }
    };
    Ok(Pattern { span, kind })
}

fn span_of(range: rustpython_parser::text_size::TextRange) -> Span {
    Span::new(u32::from(range.start()), u32::from(range.end()))
}

fn capability<T>(
    path: &Path,
    span: Span,
    code: &'static str,
    message: &str,
) -> Result<T, DiagnosticSet> {
    Err(DiagnosticSet::one(Diagnostic::new(
        code, message, path, span,
    )))
}

fn unsupported<T>(path: &Path, span: Span, message: &str) -> Result<T, DiagnosticSet> {
    capability(path, span, "RIM-CAP-001", message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path() -> &'static Path {
        Path::new("fixture.py")
    }

    #[test]
    fn parses_the_supported_statement_slice() {
        let module = parse(
            path(),
            "value = 1\nif value:\n    print(\"yes\")\nelse:\n    print(\"no\")\nwhile False:\n    value = value + 1\n",
        )
        .unwrap();
        assert_eq!(module.statements.len(), 3);
        assert!(matches!(
            module.statements[0].kind,
            StatementKind::Assign { .. }
        ));
        assert!(matches!(
            module.statements[1].kind,
            StatementKind::If { .. }
        ));
        assert!(matches!(
            module.statements[2].kind,
            StatementKind::While { .. }
        ));
    }

    #[test]
    fn keyword_print_falls_back_to_the_generic_call_path() {
        let module = parse(path(), "print(\"value\", end=\"\")\n").unwrap();
        let StatementKind::Expression(Expression {
            kind: ExpressionKind::Call { parts, .. },
            ..
        }) = &module.statements[0].kind
        else {
            panic!("keyword print should remain an ordinary call expression");
        };
        assert!(matches!(
            parts.as_slice(),
            [
                CallPart::Positional(_),
                CallPart::Keyword { name, .. }
            ] if name == "end"
        ));
    }

    #[test]
    fn gate8_with_syntax_preserves_items_targets_and_async_boundary() {
        let module = parse(path(), "with first as value, second:\n    print(value)\n").unwrap();
        let StatementKind::With { items, body, .. } = &module.statements[0].kind else {
            panic!("expected a with statement");
        };
        assert_eq!(items.len(), 2);
        assert!(items[0].target.is_some());
        assert!(items[1].target.is_none());
        assert_eq!(body.len(), 1);
        assert!(module.statements[0].span.end > module.statements[0].span.start);

        let async_module = parse(
            path(),
            "async def main():\n    async with manager:\n        value = 1\n",
        )
        .unwrap();
        let StatementKind::FunctionDef { body, is_async, .. } = &async_module.statements[0].kind
        else {
            panic!("expected async function");
        };
        assert!(*is_async);
        let StatementKind::With { is_async, .. } = &body[0].kind else {
            panic!("expected async with statement");
        };
        assert!(*is_async);
    }

    #[test]
    fn gate4_owned_syntax_has_stable_slice_diagnostics_and_spans() {
        for source in [
            "(value := 1)\n",
            "value: int = 1\n",
            "def f(value: int) -> int:\n    return value\n",
            "f'{value}'\n",
            "[item for item in values]\n",
            "{item for item in values}\n",
            "{item: item for item in values}\n",
            "(item for item in values)\n",
            "match value:\n    case 1 | 2 as chosen if chosen:\n        result = chosen\n    case _:\n        result = 0\n",
            "match value:\n    case [left, *middle, right]:\n        result = middle\n",
            "match value:\n    case {'key': captured, **rest}:\n        result = captured\n",
            "match value:\n    case Thing(first, named=second):\n        result = first\n",
            "name = 1\n",
            "obj.attr = 1\n",
            "items[0] = 1\n",
            "left, right = [1, 2]\n",
            "left, *tail = [1, 2]\n",
            "a and b\n",
            "a < b < c\n",
            "items[1:2]\n",
            "del items[0]\n",
            "assert value, message\n",
            "value = {'first': 1, **other, 'last': 2}\n",
            "f(1, *args, named=2, **kwargs)\n",
        ] {
            parse(path(), source).unwrap_or_else(|diagnostics| {
                panic!("supported Gate 4 baseline source failed: {source}: {diagnostics:?}")
            });
        }
    }

    #[test]
    fn recursive_assignment_targets_keep_shape_and_source_spans() {
        let module = parse(path(), "left, (middle, right), *tail = source\n").unwrap();
        let StatementKind::Assign { targets, .. } = &module.statements[0].kind else {
            panic!("expected assignment");
        };
        let [target] = targets.as_slice() else {
            panic!("expected one chained-assignment target");
        };
        let TargetKind::Sequence { elements, .. } = &target.kind else {
            panic!("expected outer sequence target");
        };
        assert_eq!(elements.len(), 3);
        assert!(matches!(elements[0].kind, TargetKind::Name(ref name) if name == "left"));
        assert!(matches!(elements[1].kind, TargetKind::Sequence { .. }));
        assert!(matches!(elements[2].kind, TargetKind::Starred(_)));
        assert!(target.span.end > target.span.start);
        assert!(
            elements
                .iter()
                .all(|element| element.span.end > element.span.start)
        );
    }

    #[test]
    fn malformed_source_has_a_parse_diagnostic() {
        let diagnostics = parse(path(), "if True\n    print(1)\n").unwrap_err();
        assert_eq!(diagnostics.as_slice()[0].code, "RIM-PARSE-001");
    }

    #[test]
    fn gate5_function_surface_preserves_decorators_annotations_and_parameter_kinds() {
        let module = parse(
            path(),
            "@outer\n@inner(1)\ndef function(a: int, /, b=2, *args, c: str=3, **kwargs) -> bool:\n    return a\n",
        )
        .unwrap();
        let StatementKind::FunctionDef {
            decorators,
            parameters,
            return_annotation,
            ..
        } = &module.statements[0].kind
        else {
            panic!("expected function definition");
        };
        assert_eq!(decorators.len(), 2);
        assert_eq!(parameters.len(), 5);
        assert_eq!(parameters[0].kind, ParameterKind::PositionalOnly);
        assert_eq!(parameters[1].kind, ParameterKind::PositionalOrKeyword);
        assert_eq!(parameters[2].kind, ParameterKind::VarArgs);
        assert_eq!(parameters[3].kind, ParameterKind::KeywordOnly);
        assert_eq!(parameters[4].kind, ParameterKind::VarKeywords);
        assert!(parameters[0].annotation.is_some());
        assert!(parameters[1].default.is_some());
        assert!(parameters[3].annotation.is_some());
        assert!(parameters[3].default.is_some());
        assert!(return_annotation.is_some());
    }
}
