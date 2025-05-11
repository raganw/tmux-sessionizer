use thiserror::Error;

/// Application-specific error type.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Directory scanning error: {0}")]
    ScannerError(String),

    #[error("Git operation error: {0}")]
    GitError(#[from] git2::Error),

    #[error("Fuzzy finder error: {0}")]
    FinderError(String),

    #[error("Session management error: {0}")]
    SessionError(String),

    #[error("Tmux command error: {0}")]
    TmuxError(#[from] tmux_interface::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// A type alias for `Result<T, AppError>` for use throughout the application.
pub type Result<T, E = AppError> = std::result::Result<T, E>;
