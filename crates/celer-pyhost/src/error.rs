use thiserror::Error;

#[derive(Debug, Error)]
pub enum PyHostError {
    #[error("Python initialization failed: {0}")]
    InitFailed(String),

    #[error("Module import failed: {0}")]
    ImportFailed(String),

    #[error("App object not found: {0}")]
    AppNotFound(String),

    #[error("ASGI dispatch failed: {0}")]
    DispatchFailed(String),

    #[error("ASGI protocol error: {0}")]
    ProtocolError(String),

    #[error("Python error: {0}")]
    PythonError(#[from] pyo3::PyErr),

    #[error("Event loop error: {0}")]
    EventLoopError(String),
}
