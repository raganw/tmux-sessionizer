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
/// This function sets up the global tracing subscriber to write logs to a
/// rotating file in the XDG data directory. It configures daily rotation.
/// The log level is set to `DEBUG` if `config.debug_mode` is true, otherwise `INFO`.
/// The level can be overridden using the `RUST_LOG` environment variable
/// (e.g., `RUST_LOG=tmux_sessionizer=trace`).
///
/// Note: `tracing-appender` handles rotation based on time intervals (daily),
/// but does not automatically limit the *number* of old log files kept.
/// Manual cleanup might be needed for strict file count limits.
///
/// # Arguments
///
/// * `config` - The application configuration, used to determine the default log level.
///
/// # Returns
///
/// * `Result<LoggerGuard>` - Returns a guard that must be kept alive for logging to work.
///                           Returns an `AppError::LoggingConfig` if setup fails
///                           (e.g., directory creation, file appender initialization).
pub fn init(config: &Config) -> Result<LoggerGuard> {
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

    // 6. Determine the default log level based on config
    let default_level = if config.debug_mode {
        Level::DEBUG
    } else {
        Level::INFO
    };
    let default_filter = format!("{}={}", APP_NAME, default_level); // e.g., "tmux_sessionizer=debug"

    // 7. Set up the EnvFilter
    // Use RUST_LOG if set, otherwise use the default level determined by debug_mode.
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_filter));

    // 8. Configure the tracing subscriber
    // Combine the file writer layer with the environment filter.
    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_writer(non_blocking_writer) // Write to the non-blocking file appender
                .with_ansi(false) // Disable ANSI colors in log files
                .with_file(true) // Include source file info
                .with_line_number(true), // Include source line number info
        )
        .with(filter) // Apply the filter
        .init(); // Set this subscriber as the global default

    // Log initialization success (this will now go to the file)
    info!(
        "Logging initialized. Log level determined by RUST_LOG or debug_mode (default: {}). Log dir: {}",
        default_level,
        log_dir.display()
    );
    if config.debug_mode {
        debug!("Debug mode enabled via configuration.");
    }

    // 9. Return the guard
    Ok(logger_guard)
}
