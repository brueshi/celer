use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("bind failed: {0}")]
    BindFailed(String),
    #[error("route not found: {method} {path}")]
    NotFound { method: String, path: String },
    #[error("handler error: {0}")]
    HandlerError(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
