//! Application error types.

/// Application result type alias.
pub type AppResult<T> = Result<T, AppError>;

/// Application error types.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Not authenticated")]
    NotAuthenticated,

    #[error("Configuration error: {0}")]
    Configuration(String),
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}
