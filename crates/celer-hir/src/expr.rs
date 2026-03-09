use serde::{Deserialize, Serialize};

use crate::types::TypeAnnotation;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FStringPart {
    Literal(String),
    Expression(Box<Expression>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comprehension {
    pub target: Box<Expression>,
    pub iter: Box<Expression>,
    pub conditions: Vec<Expression>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Keyword {
    pub name: Option<String>,
    pub value: Expression,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Mod,
    Pow,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    LShift,
    RShift,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UnaryOp {
    Neg,
    Not,
    BitNot,
    Pos,
}

/// HIR expression node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expression {
    IntLiteral(i64),
    FloatLiteral(f64),
    StringLiteral(String),
    BoolLiteral(bool),
    NoneLiteral,
    Name {
        id: String,
        ty: TypeAnnotation,
    },
    BinaryOp {
        op: BinaryOp,
        left: Box<Expression>,
        right: Box<Expression>,
        ty: TypeAnnotation,
    },
    UnaryOp {
        op: UnaryOp,
        operand: Box<Expression>,
        ty: TypeAnnotation,
    },
    Call {
        func: Box<Expression>,
        args: Vec<Expression>,
        keywords: Vec<Keyword>,
        ty: TypeAnnotation,
    },
    Attribute {
        value: Box<Expression>,
        attr: String,
        ty: TypeAnnotation,
    },
    Subscript {
        value: Box<Expression>,
        index: Box<Expression>,
        ty: TypeAnnotation,
    },
    List {
        elements: Vec<Expression>,
        ty: TypeAnnotation,
    },
    Dict {
        keys: Vec<Expression>,
        values: Vec<Expression>,
        ty: TypeAnnotation,
    },
    Tuple {
        elements: Vec<Expression>,
        ty: TypeAnnotation,
    },
    IfExpr {
        test: Box<Expression>,
        body: Box<Expression>,
        orelse: Box<Expression>,
        ty: TypeAnnotation,
    },
    Lambda {
        params: Vec<String>,
        body: Box<Expression>,
        ty: TypeAnnotation,
    },
    Await {
        value: Box<Expression>,
        ty: TypeAnnotation,
    },
    FString {
        parts: Vec<FStringPart>,
        ty: TypeAnnotation,
    },
    ListComp {
        element: Box<Expression>,
        generators: Vec<Comprehension>,
        ty: TypeAnnotation,
    },
    DictComp {
        key: Box<Expression>,
        value: Box<Expression>,
        generators: Vec<Comprehension>,
        ty: TypeAnnotation,
    },
}

impl Expression {
    /// Returns the type annotation for this expression.
    pub fn ty(&self) -> &TypeAnnotation {
        match self {
            Self::IntLiteral(_) => &TypeAnnotation::Int,
            Self::FloatLiteral(_) => &TypeAnnotation::Float,
            Self::StringLiteral(_) => &TypeAnnotation::Str,
            Self::BoolLiteral(_) => &TypeAnnotation::Bool,
            Self::NoneLiteral => &TypeAnnotation::None,
            Self::Name { ty, .. }
            | Self::BinaryOp { ty, .. }
            | Self::UnaryOp { ty, .. }
            | Self::Call { ty, .. }
            | Self::Attribute { ty, .. }
            | Self::Subscript { ty, .. }
            | Self::List { ty, .. }
            | Self::Dict { ty, .. }
            | Self::Tuple { ty, .. }
            | Self::IfExpr { ty, .. }
            | Self::Lambda { ty, .. }
            | Self::Await { ty, .. }
            | Self::FString { ty, .. }
            | Self::ListComp { ty, .. }
            | Self::DictComp { ty, .. } => ty,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_types() {
        assert_eq!(*Expression::IntLiteral(42).ty(), TypeAnnotation::Int);
        assert_eq!(
            *Expression::StringLiteral("hi".into()).ty(),
            TypeAnnotation::Str
        );
    }
}
