use thiserror::Error;

#[derive(Debug, Error)]
pub enum FlaskError {
    #[error("invalid route: {0}")]
    InvalidRoute(String),

    #[error("unsupported decorator: {0}")]
    UnsupportedDecorator(String),

    #[error("missing route handler for {method} {path}")]
    MissingHandler { method: String, path: String },
}
