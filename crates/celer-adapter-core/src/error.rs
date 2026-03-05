use thiserror::Error;

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("no framework detected in module")]
    NoFrameworkDetected,

    #[error("route extraction failed: {0}")]
    ExtractionFailed(String),

    #[error("invalid route definition: {0}")]
    InvalidRoute(String),
}
