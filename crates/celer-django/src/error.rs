use thiserror::Error;

#[derive(Debug, Error)]
pub enum DjangoError {
    #[error("no urlpatterns found in module")]
    NoUrlPatterns,

    #[error("invalid URL pattern: {0}")]
    InvalidUrlPattern(String),
}
