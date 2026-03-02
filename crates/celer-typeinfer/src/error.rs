use thiserror::Error;

#[derive(Debug, Error)]
pub enum TypeError {
    #[error("type mismatch: expected {expected}, found {found}")]
    Mismatch { expected: String, found: String },

    #[error("undefined variable: {0}")]
    UndefinedVariable(String),

    #[error("cannot infer type for: {0}")]
    InferenceFailure(String),

    #[error("incompatible types in {op}: {left} and {right}")]
    BinaryOpMismatch {
        op: String,
        left: String,
        right: String,
    },
}
