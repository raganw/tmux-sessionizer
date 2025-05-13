//! # Logging Module
//!
//! Handles the setup and configuration of application-wide logging using the `tracing` ecosystem.
//! Configures logging to a rotating file located in the appropriate XDG data directory.

use crate::config::Config;
use crate::error::{AppError, Result};
use cross_xdg::BaseDirs;
use std::fs;
use std::path::PathBuf;
use tracing::{debug, error, info, Level};
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard}; // Import NonBlocking
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

const APP_NAME: &str = "tmux-sessionizer"; // Define app name for directory/file naming

/// Represents the logging configuration and holds the guard for the non-blocking writer.
///
/// The `WorkerGuard` must be kept alive for the duration of the application
/// to ensure logs are flushed.
#[derive(Debug)]
pub struct LoggerGuard {
    _guard: WorkerGuard,
}

/// Initializes the logging system.
///
/// This function sets up the global tracing subscriber to write logs to a
/// rotating file in the XDG data directory. It configures daily rotation.
/// Note: `tracing-appender` handles rotation based on time intervals (daily),
/// but does not automatically limit the *number* of old log files kept.
/// Manual cleanup might be needed for strict file count limits.
///
/// # Arguments
///
/// * `config` - The application configuration, used to determine the log level.
///
/// # Returns
///
/// * `Result<LoggerGuard>` - Returns a guard that must be kept alive for logging.
///                           Returns an error if setup fails (e.g., directory creation).
pub fn init(_config: &Config) -> Result<LoggerGuard> {
    // 1. Determine the XDG data directory
    let base_dirs = BaseDirs::new()
        .map_err(|e| AppError::LoggingConfig(format!("Failed to get XDG base dirs: {}", e)))?;
    let data_home = base_dirs.data_home();

    // 2. Define the application-specific log directory path
    let log_dir = data_home.join(APP_NAME);

    // 3. Create the log directory if it doesn't exist
    fs::create_dir_all(&log_dir).map_err(|e| {
        AppError::LoggingConfig(format!(
            "Failed to create log directory '{}': {}",
            log_dir.display(),
            e
        ))
    })?;

    // 4. Configure the rolling file appender
    // Rotate daily and write logs to APP_NAME.log, APP_NAME.log.YYYY-MM-DD, etc.
    // The library handles file creation and rotation based on the date.
    // It does *not* automatically limit the number of files kept to 2.
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY) // Rotate daily
        .filename_prefix(APP_NAME) // Log file prefix (e.g., tmux-sessionizer)
        .filename_suffix("log") // Log file suffix (e.g., .log)
        .build(&log_dir) // Directory to store log files
        .map_err(|e| {
            AppError::LoggingConfig(format!(
                "Failed to initialize rolling file appender in '{}': {}",
                log_dir.display(),
                e
            ))
        })?;

    // 5. Create a non-blocking writer for performance.
    // The guard must be kept alive to ensure logs are flushed.
    let (non_blocking_writer, guard) = tracing_appender::non_blocking(file_appender);

    // Store the guard in the struct to be returned.
    let logger_guard = LoggerGuard { _guard: guard };

    // Debug logs about paths (will only appear if a subscriber is eventually set up correctly)
    debug!("Log directory set to: {}", log_dir.display());
    let log_file_path = log_dir.join(format!("{}.log", APP_NAME)); // Example path for logging
    debug!("Primary log file target: {}", log_file_path.display());

    // TODO: Set up the actual tracing subscriber using non_blocking_writer
    // This part will be implemented in the next step.
    // For now, return the guard. The subscriber setup is pending.

    // Placeholder return until subscriber is set up in the next step
    // Ok(logger_guard) // This will be the final return, but need subscriber setup first.

    unimplemented!(
        "Logging initialization (subscriber setup using non_blocking_writer) not yet implemented"
    );

    // Return the guard (even though the subscriber isn't fully set up yet)
    // Ok(logger_guard) // This line is commented out as the function still has unimplemented parts.
}
