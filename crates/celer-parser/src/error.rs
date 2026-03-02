use thiserror::Error;

/// Errors produced during Python-to-HIR conversion.
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("syntax error at line {line}: {msg}")]
    SyntaxError { line: usize, msg: String },

    #[error("unsupported Python feature: {0}")]
    UnsupportedFeature(String),

    #[error("HIR conversion error: {0}")]
    ConversionError(String),
}

impl From<rustpython_parser::ParseError> for ParseError {
    fn from(err: rustpython_parser::ParseError) -> Self {
        Self::SyntaxError {
            line: 0,
            msg: err.to_string(),
        }
    }
}
