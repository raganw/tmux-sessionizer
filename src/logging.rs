//! # Logging Module
//!
//! Handles the setup and configuration of application-wide logging using the `tracing` ecosystem.
//! Configures logging to a rotating file located in the appropriate XDG data directory.

use crate::error::{AppError, Result};
use cross_xdg::BaseDirs;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use tracing::{Level, Subscriber, info};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

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
    default_level: Level,
) -> Result<(WorkerGuard, impl Subscriber + Send + Sync, PathBuf)> {
    // 1. Determine the XDG data directory
    let base_dirs = BaseDirs::new()
        .map_err(|e| AppError::LoggingConfig(format!("Failed to get XDG base dirs: {e}")))?;
    let data_home = base_dirs.data_home();

    // 2. Define the application-specific log directory path using the module's APP_NAME
    let log_dir_path = data_home.join(APP_NAME);

    // 3. Create the log directory if it doesn't exist
    fs::create_dir_all(&log_dir_path).map_err(|e| {
        AppError::LoggingConfig(format!(
            "Failed to create log directory '{}': {}",
            log_dir_path.display(),
            e
        ))
    })?;

    // 4. Configure the rolling file appender
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(APP_NAME) // Use module's APP_NAME for file prefix
        .filename_suffix("log")
        .build(&log_dir_path)
        .map_err(|e| {
            AppError::LoggingConfig(format!(
                "Failed to initialize rolling file appender in '{}': {}",
                log_dir_path.display(),
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

    Ok((worker_guard, subscriber, log_dir_path))
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
pub fn init_global_tracing(level: &str) -> Result<WorkerGuard> {
    let log_level = Level::from_str(level)
        .map_err(|_| AppError::LoggingConfig(format!("Invalid log level string: {}", level)))?;

    let (guard, subscriber, log_dir_path) = init_file_subscriber(log_level)?;
    tracing::subscriber::set_global_default(subscriber).map_err(|e| {
        AppError::LoggingConfig(format!("Failed to set global default subscriber: {}", e))
    })?;
    eprintln!(
        "Setting global tracing subscriber to file appender in '{}'",
        log_dir_path.display()
    );
    // This log message will go to the newly set global subscriber (i.e., the file)
    info!(
        "initialized global tracing. Log directory: {}",
        log_dir_path.display()
    );
    Ok(guard)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::thread;
    use std::time::Duration;
    use tempfile::tempdir;
    use tracing::subscriber::DefaultGuard as TracingDefaultGuard;

    /// Creates a tracing/logging subscriber that is valid until the guards are dropped.
    ///
    /// This function sets up logging to a file in the XDG data directory.
    /// The format layer logs spans/events in plain text, without color, one event per line.
    /// This is primarily useful for setting a temporary default subscriber in unit tests.
    ///
    /// # Arguments
    ///
    /// * `level` - A string slice representing the desired default log level (e.g., "info", "debug").
    ///
    /// # Returns
    ///
    /// * `Result<(WorkerGuard, TracingDefaultGuard)>` - Returns the worker guard for the
    ///   non-blocking file appender and the guard for the default subscriber.
    ///   Returns `AppError::LoggingConfig` if setup fails.
    pub fn init_tracing(level: &str) -> Result<(WorkerGuard, TracingDefaultGuard)> {
        let log_level = Level::from_str(level)
            .map_err(|_| AppError::LoggingConfig(format!("Invalid log level string: {}", level)))?;

        let (guard, subscriber, log_dir_path) = init_file_subscriber(log_level)?;
        let subscriber_guard = tracing::subscriber::set_default(subscriber);
        // This log message will go to the newly set default subscriber (i.e., the file)
        info!(
            "initialized tracing. Log directory: {}",
            log_dir_path.display()
        );
        Ok((guard, subscriber_guard))
    }

    // Helper function to get the expected log directory path based on a temporary data home.
    fn get_expected_log_dir(temp_data_home: &Path) -> PathBuf {
        temp_data_home.join(APP_NAME)
    }

    #[test]
    #[serial]
    fn test_xdg_directory_determination_and_creation() {
        let temp_dir = tempdir().expect("Failed to create temp dir for XDG test");
        let temp_data_home = temp_dir.path().to_path_buf();
        let original_xdg_data_home = env::var_os("XDG_DATA_HOME");

        // Set XDG_DATA_HOME for this test
        unsafe {
            env::set_var("XDG_DATA_HOME", &temp_data_home);
        }

        let expected_log_dir = get_expected_log_dir(&temp_data_home);

        // Ensure the directory does not exist before the call within init
        if expected_log_dir.exists() {
            fs::remove_dir_all(&expected_log_dir)
                .expect("Failed to clean up pre-existing test log dir");
        }

        // Call init_tracing to trigger directory creation logic
        let (_worker_guard, _subscriber_guard) =
            init_tracing("info").expect("Logger initialization failed");

        // Restore original environment variable
        match original_xdg_data_home {
            Some(val) => unsafe { env::set_var("XDG_DATA_HOME", val) },
            None => unsafe { env::remove_var("XDG_DATA_HOME") },
        }

        assert!(
            expected_log_dir.exists(),
            "Log directory '{}' should have been created",
            expected_log_dir.display()
        );
        assert!(
            expected_log_dir.is_dir(),
            "Log directory path '{}' should point to a directory",
            expected_log_dir.display()
        );

        // Clean up the created directory
        fs::remove_dir_all(&expected_log_dir).expect("Failed to clean up test log dir");
    }

    #[test]
    #[serial] // Modifies environment variables and global tracing state
    fn test_log_file_creation() {
        let temp_dir = tempdir().expect("Failed to create temp dir for log file test");
        let temp_data_home = temp_dir.path().to_path_buf();
        let original_xdg_data_home = env::var_os("XDG_DATA_HOME");
        unsafe {
            env::set_var("XDG_DATA_HOME", &temp_data_home);
        }

        let expected_log_dir = get_expected_log_dir(&temp_data_home);
        println!("Expected log directory: {}", expected_log_dir.display());
        // Matches the pattern set in init: {prefix}.{suffix}
        let expected_log_file = expected_log_dir.join(format!("{}.log", APP_NAME));

        println!("Expected log file path: {}", expected_log_file.display());
        // Ensure clean state
        if expected_log_dir.exists() {
            println!(
                "Cleaning up pre-existing log directory: {}",
                expected_log_dir.display()
            );
            fs::remove_dir_all(&expected_log_dir)
                .expect("Failed to clean up pre-existing test log dir");
        }

        // Initialize logging using init_tracing
        let (_worker_guard, subscriber_guard) =
            init_tracing("info").expect("Logger initialization failed");

        tracing::info!("Test message for log file creation.");

        drop(subscriber_guard); // Drop subscriber guard first
        // _worker_guard will be dropped here, ensuring flush
        thread::sleep(Duration::from_millis(200)); // Allow more time for flush

        match original_xdg_data_home {
            Some(val) => unsafe { env::set_var("XDG_DATA_HOME", val) },
            None => unsafe { env::remove_var("XDG_DATA_HOME") },
        }

        assert!(
            expected_log_dir.exists(),
            "Log directory should exist after init"
        );
        assert!(
            expected_log_file.exists(),
            "Log file '{}' should exist after init and logging. Content: {:?}",
            expected_log_file.display(),
            fs::read_to_string(&expected_log_file)
                .unwrap_or_else(|_| "Error reading file".to_string())
        );
        assert!(
            expected_log_file.is_file(),
            "Log file path '{}' should be a file",
            expected_log_file.display()
        );

        let metadata = fs::metadata(&expected_log_file).expect("Failed to get log file metadata");
        assert!(
            metadata.len() > 0,
            "Log file should not be empty after logging"
        );
        let content = fs::read_to_string(&expected_log_file).unwrap();
        assert!(content.contains("Test message for log file creation."));
        assert!(content.contains("initialized tracing. Log directory:"));

        // Clean up
        fs::remove_dir_all(&expected_log_dir).expect("Failed to clean up test log dir");
    }

    #[test]
    #[serial] // Modifies environment variables and global tracing state
    fn test_debug_mode_level_setting() {
        // Test with debug_mode = true
        unsafe {
            env::remove_var("RUST_LOG");
        } // Ensure RUST_LOG is not set to interfere
        let temp_dir_debug = tempdir().expect("Failed temp dir for debug test");
        let temp_data_home_debug = temp_dir_debug.path().to_path_buf();
        println!(
            "Temporary data home for debug test: {}",
            temp_data_home_debug.display()
        );
        let original_xdg_data_home = env::var_os("XDG_DATA_HOME");
        println!(
            "Original XDG_DATA_HOME: {:?}",
            original_xdg_data_home
                .as_ref()
                .map(|s| s.to_string_lossy())
                .unwrap_or_else(|| "Not set".into())
        );
        unsafe {
            env::set_var("XDG_DATA_HOME", &temp_data_home_debug);
        }

        let (_w_guard_debug, s_guard_debug) =
            init_tracing("debug").expect("Logger init failed for debug test");
        tracing::debug!("This debug message should be logged in debug mode.");
        drop(s_guard_debug);
        thread::sleep(Duration::from_millis(100));

        let log_file_debug =
            get_expected_log_dir(&temp_data_home_debug).join(format!("{}.log", APP_NAME));
        let content_debug =
            fs::read_to_string(&log_file_debug).expect("Failed to read debug log file");
        assert!(
            content_debug.contains("initialized tracing. Log directory:"),
            "Log init message should be present"
        );
        // The filter string itself is "tmux_sessionizer=debug", not part of the log message from init_tracing
        // We check for the effect:
        assert!(
            content_debug.contains("This debug message should be logged"),
            "Debug message missing in debug mode"
        );

        // Test with "info" level
        unsafe {
            env::remove_var("RUST_LOG");
        } // Ensure RUST_LOG is not set
        let temp_dir_info = tempdir().expect("Failed temp dir for info test");
        let temp_data_home_info = temp_dir_info.path().to_path_buf();
        unsafe {
            env::set_var("XDG_DATA_HOME", &temp_data_home_info);
        }

        let (_w_guard_info, s_guard_info) =
            init_tracing("info").expect("Logger init failed for info test");
        tracing::debug!("This debug message should NOT be logged in info mode.");
        tracing::info!("This info message should be logged in info mode.");
        drop(s_guard_info);
        thread::sleep(Duration::from_millis(100));

        let log_file_info =
            get_expected_log_dir(&temp_data_home_info).join(format!("{}.log", APP_NAME));
        let content_info =
            fs::read_to_string(&log_file_info).expect("Failed to read info log file");
        assert!(
            content_info.contains("initialized tracing. Log directory:"),
            "Log init message should be present"
        );
        assert!(
            content_info.contains("This info message should be logged"),
            "Info message missing in info mode"
        );
        assert!(
            !content_info.contains("This debug message should NOT be logged"),
            "Debug message unexpectedly present in info mode"
        );

        match original_xdg_data_home {
            Some(val) => unsafe { env::set_var("XDG_DATA_HOME", val) },
            None => unsafe { env::remove_var("XDG_DATA_HOME") },
        }

        // Clean up
        fs::remove_dir_all(get_expected_log_dir(&temp_data_home_debug)).ok();
        fs::remove_dir_all(get_expected_log_dir(&temp_data_home_info)).ok();
    }

    // Note on Rotation Testing:
    // Testing the actual file rotation (e.g., keeping only 2 files after several days)
    // is complex and brittle in unit tests. It would require manipulating time or
    // relying heavily on the internal implementation details of `tracing-appender`.
    // We trust that configuring `Rotation::DAILY` correctly instructs the library
    // to perform daily rotation. These tests focus on verifying the initial setup,
    // directory/file creation, and log level configuration based on `debug_mode`.
}
