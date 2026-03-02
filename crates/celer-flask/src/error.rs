use thiserror::Error;

#[derive(Debug, Error)]
pub enum FlaskError {
    #[error("Flask adapter not yet implemented (Phase 2)")]
    NotImplemented,

    #[error("invalid route: {0}")]
    InvalidRoute(String),
}
