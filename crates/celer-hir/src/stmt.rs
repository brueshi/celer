use serde::{Deserialize, Serialize};

use crate::expr::Expression;
use crate::func::Function;
use crate::types::TypeAnnotation;

/// HIR statement node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Statement {
    Assign {
        target: String,
        annotation: Option<TypeAnnotation>,
        value: Expression,
    },
    AugAssign {
        target: String,
        op: crate::expr::BinaryOp,
        value: Expression,
    },
    Return {
        value: Option<Expression>,
    },
    If {
        test: Expression,
        body: Vec<Statement>,
        orelse: Vec<Statement>,
    },
    While {
        test: Expression,
        body: Vec<Statement>,
    },
    For {
        target: String,
        iter: Expression,
        body: Vec<Statement>,
    },
    FunctionDef(Function),
    ClassDef {
        name: String,
        bases: Vec<String>,
        methods: Vec<Function>,
        fields: Vec<(String, TypeAnnotation)>,
    },
    Import {
        module: String,
        names: Vec<(String, Option<String>)>,
    },
    ImportFrom {
        module: String,
        names: Vec<(String, Option<String>)>,
    },
    Expr(Expression),
    Pass,
    Break,
    Continue,
    Raise {
        value: Option<Expression>,
    },
    Assert {
        test: Expression,
        msg: Option<Expression>,
    },
}
