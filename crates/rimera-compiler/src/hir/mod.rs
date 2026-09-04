use crate::core::Span;
use rimera_abi::RParameterKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanOperator {
    And,
    Or,
}
pub use rimera_abi::{
    RBinaryOperator as BinaryOperator, RCompareOperator as CompareOperator,
    RUnaryOperator as UnaryOperator,
};

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
        binding: Binding,
        type_params: Vec<TypeParameter>,
        decorators: Vec<Expression>,
        parameters: Vec<Parameter>,
        return_annotation: Option<Expression>,
        body: Vec<Statement>,
        locals: Vec<String>,
        cells: Vec<String>,
        free: Vec<String>,
    },
    ClassDef {
        name: String,
        binding: Binding,
        type_params: Vec<TypeParameter>,
        decorators: Vec<Expression>,
        bases: Vec<Expression>,
        metaclass: Option<Expression>,
        keywords: Vec<(String, Expression)>,
        body: Vec<ClassMember>,
    },
    TypeAlias {
        name: String,
        binding: Binding,
        type_params: Vec<TypeParameter>,
        value: Expression,
    },
    Return {
        value: Option<Expression>,
    },
    Import {
        aliases: Vec<ImportAlias>,
    },
    Break,
    Continue,
    Expression(Expression),
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
    },
    Match {
        subject: Expression,
        cases: Vec<MatchCase>,
    },
}

#[derive(Debug, Clone)]
pub struct ImportAlias {
    pub module: String,
    pub bind_name: String,
    pub binding: Binding,
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
    Capture {
        name: String,
        binding: Binding,
    },
    Wildcard,
    As {
        pattern: Box<Pattern>,
        name: String,
        binding: Binding,
    },
    Or(Vec<Pattern>),
    Sequence(Vec<Pattern>),
    Star(Option<(String, Binding)>),
    Mapping {
        keys: Vec<Expression>,
        patterns: Vec<Pattern>,
        rest: Option<(String, Binding)>,
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
    Name {
        name: String,
        binding: Binding,
    },
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
pub enum ClassMember {
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
    Raise {
        exception: Option<Expression>,
        cause: Option<Expression>,
    },
    Try {
        body: Vec<ClassMember>,
        handlers: Vec<ClassExceptionHandler>,
        else_body: Vec<ClassMember>,
        finally_body: Vec<ClassMember>,
        is_star: bool,
    },
    Import {
        aliases: Vec<ImportAlias>,
    },
    ClassDef {
        name: String,
        binding: Binding,
        type_params: Vec<TypeParameter>,
        decorators: Vec<Expression>,
        bases: Vec<Expression>,
        metaclass: Option<Expression>,
        keywords: Vec<(String, Expression)>,
        body: Vec<ClassMember>,
    },
    TypeAlias {
        name: String,
        binding: Binding,
        type_params: Vec<TypeParameter>,
        value: Expression,
    },
    Break,
    Continue,
    FunctionDef {
        span: Span,
        name: String,
        binding: Binding,
        type_params: Vec<TypeParameter>,
        decorators: Vec<Expression>,
        uses_zero_argument_super: bool,
        parameters: Vec<Parameter>,
        return_annotation: Option<Expression>,
        body: Vec<Statement>,
        locals: Vec<String>,
        cells: Vec<String>,
        free: Vec<String>,
    },
    If {
        condition: Expression,
        then_body: Vec<ClassMember>,
        else_body: Vec<ClassMember>,
    },
    While {
        condition: Expression,
        body: Vec<ClassMember>,
    },
    For {
        target: Target,
        iterable: Expression,
        body: Vec<ClassMember>,
        else_body: Vec<ClassMember>,
    },
    Expression(Expression),
    Print(Vec<Expression>),
}

#[derive(Debug, Clone)]
pub struct ExceptionHandler {
    pub exception_type: Option<Expression>,
    pub name: Option<(String, Binding)>,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub struct ClassExceptionHandler {
    pub exception_type: Option<Expression>,
    pub name: Option<(String, Binding)>,
    pub body: Vec<ClassMember>,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub kind: RParameterKind,
    pub default: Option<Expression>,
    pub annotation: Option<Expression>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Binding {
    Local,
    Cell,
    Free,
    Global,
    /// Dynamic class-body name: prepared namespace first, then globals/builtins.
    ClassName,
    /// Class-body free name: prepared namespace first, then an enclosing cell.
    ClassFree,
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
    /// `None` for the first clause because its iterator is created in the
    /// containing scope and passed to the hidden comprehension function.
    pub iterable: Option<Expression>,
    pub filters: Vec<Expression>,
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
    Length {
        value: Box<Expression>,
    },
    Range {
        start: Box<Expression>,
        stop: Box<Expression>,
        step: Box<Expression>,
    },
    Lambda {
        parameters: Vec<Parameter>,
        body: Box<Expression>,
        locals: Vec<String>,
        cells: Vec<String>,
        free: Vec<String>,
    },
    Name {
        name: String,
        binding: Binding,
    },
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
        binding: Binding,
        value: Box<Expression>,
    },
    Yield {
        value: Option<Box<Expression>>,
    },
    YieldFrom {
        value: Box<Expression>,
    },
    Comprehension {
        kind: ComprehensionKind,
        outer_iterable: Box<Expression>,
        element: Box<Expression>,
        key: Option<Box<Expression>>,
        clauses: Vec<ComprehensionClause>,
        locals: Vec<String>,
        cells: Vec<String>,
        free: Vec<String>,
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
