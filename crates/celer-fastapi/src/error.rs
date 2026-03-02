use thiserror::Error;

#[derive(Debug, Error)]
pub enum FastApiError {
    #[error("invalid route definition: {0}")]
    InvalidRoute(String),

    #[error("unsupported decorator: {0}")]
    UnsupportedDecorator(String),

    #[error("missing route handler for {method} {path}")]
    MissingHandler { method: String, path: String },
}
