use thiserror::Error;

#[derive(Debug, Error)]
pub enum HirError {
    #[error("unsupported syntax: {0}")]
    UnsupportedSyntax(String),

    #[error("invalid type annotation: {0}")]
    InvalidType(String),

    #[error("duplicate definition: {0}")]
    DuplicateDefinition(String),
}
