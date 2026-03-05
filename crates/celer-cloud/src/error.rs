use thiserror::Error;

#[derive(Debug, Error)]
pub enum CloudError {
    #[error("bind failed: {0}")]
    BindFailed(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("source too large: {size} bytes (max: {max})")]
    SourceTooLarge { size: usize, max: usize },

    #[error("compilation failed: {0}")]
    CompilationFailed(String),

    #[error("compilation timed out after {0} seconds")]
    Timeout(u64),

    #[error("job not found: {0}")]
    JobNotFound(String),

    #[error("invalid module name: {0}")]
    InvalidModuleName(String),

    #[error("job not complete")]
    JobNotComplete,
}
