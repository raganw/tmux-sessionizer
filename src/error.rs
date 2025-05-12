//! Defines the central error type (`AppError`) for the application and a standard `Result` type.
//!
//! This module uses the `thiserror` crate to derive the `std::error::Error` trait
//! and provide convenient error handling throughout the application. It aggregates
//! errors from various sources like I/O, Git operations, configuration, etc.

use regex;
use std::io;
use std::path::PathBuf;
use thiserror::Error;
use toml;

// Define new error enums for configuration and path validation issues
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Could not determine the system config directory")]
    CannotDetermineConfigDir,

    #[error("Failed to read configuration file '{path}': {source}")]
    FileReadError { path: PathBuf, source: io::Error },

    #[error("Failed to parse configuration file '{path}': {source}")]
    FileParseError {
        path: PathBuf,
        source: toml::de::Error,
    },

    #[error("Invalid configuration value: {message}")]
    Validation { message: String }, // General validation error

    #[error("Invalid regex pattern '{pattern}' in configuration: {source}")]
    InvalidRegex {
        pattern: String,
        source: regex::Error,
    },

    #[error("Path validation failed: {0}")]
    InvalidPath(#[from] PathValidationError),
}

#[derive(Debug, Error)]
pub enum PathValidationError {
    #[error("Path does not exist: '{path}'")]
    DoesNotExist { path: PathBuf },

    #[error("Path is not a directory: '{path}'")]
    NotADirectory { path: PathBuf },

    #[error("Permission denied for path: '{path}'")]
    PermissionDenied { path: PathBuf }, // Note: May be hard to distinguish reliably from other errors

    #[error("Filesystem error checking path '{path}': {source}")]
    FilesystemError { path: PathBuf, source: io::Error },
}

/// The primary error type used throughout the `tmux-sessionizer` application.
///
/// This enum consolidates various error kinds that can occur during the application's
/// execution, including configuration issues, filesystem scanning problems, Git operations,
/// fuzzy finder interactions, tmux commands, and general I/O errors.
#[derive(Debug, Error)]
pub enum AppError {
    /// Errors related to application configuration loading or validation.
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError), // Changed from String to ConfigError

    /// Errors encountered during directory scanning or processing.
    #[error("Directory scanning error: {0}")]
    _Scanner(String),

    /// Errors originating from Git operations (e.g., opening repositories, listing worktrees).
    /// Wraps `git2::Error`.
    #[error("Git operation error: {0}")]
    Git(#[from] git2::Error),

    /// General internal errors, often used for unexpected conditions or wrapping errors
    /// from libraries without specific variants (via `anyhow`).
    #[error("Internal error: {0}")]
    Anyhow(#[from] anyhow::Error),

    /// Errors related to the fuzzy finder interface (e.g., Skim library errors).
    #[error("Fuzzy finder error: {0}")]
    Finder(String),

    /// Errors related to tmux session management (e.g., creating, switching sessions).
    #[error("Session management error: {0}")]
    Session(String),

    /// Errors originating from executing tmux commands via the `tmux_interface` crate.
    /// Wraps `tmux_interface::Error`.
    #[error("Tmux command error: {0}")]
    Tmux(#[from] tmux_interface::Error),

    /// Standard I/O errors encountered during file operations.
    /// Wraps `std::io::Error`.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Errors related to thread synchronization, specifically Mutex poisoning.
    #[error("Mutex synchronization error: {0}")]
    MutexError(String),
}

/// A type alias for `Result<T, AppError>` providing a convenient shorthand
/// for functions that return the application's standard error type.
pub type Result<T, E = AppError> = std::result::Result<T, E>;
