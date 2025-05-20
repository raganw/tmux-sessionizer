//! # Logging Module
//!
//! Handles the setup and configuration of application-wide logging using the `tracing` ecosystem.
//! Configures logging to a rotating file located in the appropriate XDG data directory.

use crate::error::{AppError, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tracing::{Level, Subscriber, info};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};

const APP_NAME: &str = "tmux-sessionizer";

/// Initializes a file-based tracing subscriber.
///
/// This private helper function sets up a rolling file appender in the XDG data directory
/// under an application-specific subdirectory. It configures daily rotation.
///
/// # Arguments
///
/// * `default_level` - The default `tracing::Level` to use if `RUST_LOG` is not set.
///
/// # Returns
///
/// * `Result<(WorkerGuard, impl Subscriber + Send + Sync, PathBuf)>` -
///   Returns the worker guard for the non-blocking appender, the configured subscriber,
///   and the path to the log directory.
///   Returns `AppError::LoggingConfig` if setup fails.
fn init_file_subscriber(
    log_directory: &Path,
    default_level: Level,
) -> Result<(WorkerGuard, impl Subscriber + Send + Sync, PathBuf)> {
    // 3. Create the log directory if it doesn't exist
    fs::create_dir_all(log_directory).map_err(|e| {
        AppError::LoggingConfig(format!(
            "Failed to create log directory '{}': {}",
            log_directory.display(),
            e
        ))
    })?;

    // 4. Configure the rolling file appender
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(APP_NAME) // Use module's APP_NAME for file prefix
        .filename_suffix("log")
        .build(log_directory)
        .map_err(|e| {
            AppError::LoggingConfig(format!(
                "Failed to initialize rolling file appender in '{}': {}",
                log_directory.display(),
                e
            ))
        })?;

    // 5. Create a non-blocking writer for performance.
    let (non_blocking_writer, worker_guard) = tracing_appender::non_blocking(file_appender);

    // 6. Configure the tracing subscriber layers
    let subscriber = tracing_subscriber::fmt()
        .with_writer(non_blocking_writer)
        .with_ansi(false)
        .with_file(true)
        .with_line_number(true)
        .with_max_level(default_level)
        .finish();

    Ok((worker_guard, subscriber, log_directory.to_path_buf()))
}

/// Creates and sets a global tracing/logging subscriber.
///
/// This function initializes logging to a file in the XDG data directory and sets it
/// as the global default subscriber for the application.
///
/// # Arguments
///
/// * `level` - A string slice representing the desired default log level (e.g., "info", "debug").
///
/// # Returns
///
/// * `Result<WorkerGuard>` - Returns the worker guard for the non-blocking file appender.
///   This guard must be kept alive for the duration of the application to ensure logs are flushed.
///   Returns `AppError::LoggingConfig` if setup fails.
pub fn init_global_tracing(log_directory: &Path, level: &str) -> Result<WorkerGuard> {
    let log_level = Level::from_str(level)
        .map_err(|_| AppError::LoggingConfig(format!("Invalid log level string: {level}")))?;

    let (guard, subscriber, log_dir_path) = init_file_subscriber(log_directory, log_level)?;
    tracing::subscriber::set_global_default(subscriber).map_err(|e| {
        AppError::LoggingConfig(format!("Failed to set global default subscriber: {e}"))
    })?;
    // This log message will go to the newly set global subscriber (i.e., the file)
    info!(
        "initialized global tracing. Log directory: {}",
        log_dir_path.display()
    );
    Ok(guard)
}

#[cfg(test)]
mod tests;
