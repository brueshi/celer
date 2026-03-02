use thiserror::Error;

#[derive(Debug, Error)]
pub enum DjangoError {
    #[error("Django adapter not yet implemented (Phase 2)")]
    NotImplemented,

    #[error("invalid URL pattern: {0}")]
    InvalidUrlPattern(String),
}
