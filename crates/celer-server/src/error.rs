use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("bind failed: {0}")]
    BindFailed(String),
    #[error("route not found: {method} {path}")]
    NotFound { method: String, path: String },
    #[error("handler error: {0}")]
    HandlerError(String),
    #[error("body error: {0}")]
    BodyError(String),
    #[error("ASGI error: {0}")]
    AsgiError(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
