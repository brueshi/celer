use thiserror::Error;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("module not found: {0}")]
    ModuleNotFound(String),

    #[error("execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Python error: {0}")]
    PythonError(String),

    #[error("initialization failed: {0}")]
    InitFailed(String),
}

impl From<pyo3::PyErr> for RuntimeError {
    fn from(err: pyo3::PyErr) -> Self {
        Self::PythonError(err.to_string())
    }
}
