use serde::{Deserialize, Serialize};

use crate::stmt::Statement;
use crate::types::TypeAnnotation;

/// A typed function parameter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub annotation: TypeAnnotation,
    pub default: Option<crate::expr::Expression>,
}

/// HIR representation of a Python function definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    pub params: Vec<Parameter>,
    pub return_type: TypeAnnotation,
    pub body: Vec<Statement>,
    pub decorators: Vec<String>,
    pub is_async: bool,
}
