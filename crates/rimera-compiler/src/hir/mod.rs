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
    FunctionDef {
        name: String,
        binding: Binding,
        decorators: Vec<Expression>,
        parameters: Vec<Parameter>,
        body: Vec<Statement>,
        locals: Vec<String>,
        cells: Vec<String>,
        free: Vec<String>,
    },
    ClassDef {
        name: String,
        binding: Binding,
        decorators: Vec<Expression>,
        bases: Vec<Expression>,
        metaclass: Option<Expression>,
        keywords: Vec<(String, Expression)>,
        body: Vec<ClassMember>,
    },
    Return {
        value: Option<Expression>,
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
        name: String,
        value: Expression,
    },
    AugAssign {
        name: String,
        op: BinaryOperator,
        value: Expression,
    },
    Delete {
        name: String,
    },
    ItemAssign {
        collection: Expression,
        index: Expression,
        value: Expression,
    },
    AttributeAssign {
        receiver: Expression,
        name: String,
        value: Expression,
    },
    AttributeDelete {
        receiver: Expression,
        name: String,
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
    ClassDef {
        name: String,
        decorators: Vec<Expression>,
        bases: Vec<Expression>,
        metaclass: Option<Expression>,
        keywords: Vec<(String, Expression)>,
        body: Vec<ClassMember>,
    },
    Break,
    Continue,
    FunctionDef {
        name: String,
        decorators: Vec<Expression>,
        uses_zero_argument_super: bool,
        parameters: Vec<Parameter>,
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
    pub name: Option<String>,
    pub body: Vec<ClassMember>,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub kind: RParameterKind,
    pub default: Option<Expression>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Binding {
    Local,
    Cell,
    Free,
    Global,
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
        op: CompareOperator,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Call {
        callable: Box<Expression>,
        parts: Vec<CallPart>,
    },
}
