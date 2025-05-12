use thiserror::Error;

/// Application-specific error type.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("Configuration error: {0}")]
    _Config(String), // TODO: Remove underscore prefix if used

    #[error("Directory scanning error: {0}")]
    _Scanner(String), // TODO: Remove underscore prefix if used

    #[error("Git operation error: {0}")]
    Git(#[from] git2::Error),

    #[error("Internal error: {0}")]
    Anyhow(#[from] anyhow::Error),

    #[error("Fuzzy finder error: {0}")]
    Finder(String),

    #[error("Session management error: {0}")]
    Session(String),

    #[error("Tmux command error: {0}")]
    Tmux(#[from] tmux_interface::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Mutex synchronization error: {0}")]
    MutexError(String),
}

/// A type alias for `Result<T, AppError>` for use throughout the application.
pub type Result<T, E = AppError> = std::result::Result<T, E>;
