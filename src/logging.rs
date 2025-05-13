//! # Logging Module
//!
//! Handles the setup and configuration of application-wide logging using the `tracing` ecosystem.
//! Configures logging to a rotating file located in the appropriate XDG data directory.

use crate::config::Config;
use crate::error::{AppError, Result};
use cross_xdg::BaseDirs;
use std::fs;
use tracing::{Level, debug, info}; // Removed 'error'
use tracing_appender::non_blocking::WorkerGuard; // Import NonBlocking, removed NonBlocking
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

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
///   Returns an `AppError::LoggingConfig` if setup fails
///   (e.g., directory creation, file appender initialization).
pub fn init(config: &Config) -> Result<LoggerGuard> {
    // 1. Determine the XDG data directory
    let base_dirs = BaseDirs::new()
        .map_err(|e| AppError::LoggingConfig(format!("Failed to get XDG base dirs: {e}")))?;
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
    let default_filter = format!("{APP_NAME}={default_level}"); // e.g., "tmux_sessionizer=debug"

    // 7. Set up the EnvFilter
    // Use RUST_LOG if set, otherwise use the default level determined by debug_mode.
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

    // 8. Configure the tracing subscriber
    // Combine the file writer layer with the environment filter.
    // Use try_init() to avoid panicking if a global subscriber is already set (common in tests).
    let init_result = tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_writer(non_blocking_writer) // Write to the non-blocking file appender
                .with_ansi(false) // Disable ANSI colors in log files
                .with_file(true) // Include source file info
                .with_line_number(true), // Include source line number info
        )
        .with(filter) // Apply the filter
        .try_init(); // Set this subscriber as the global default, handle error if already set

    if let Err(e) = init_result {
        // Log a warning if initialization failed, likely because a subscriber already exists.
        // This is often acceptable in test scenarios or if init is called multiple times.
        eprintln!(
            "WARN: Failed to set global default tracing subscriber: {}. Logging might not be fully configured.",
            e
        );
        // Optionally, use tracing::warn! here, but it might not work if the subscriber failed completely.
        // tracing::warn!("Failed to set global default tracing subscriber: {}. Logging might not be fully configured.", e);
    }

    // Log initialization success (this will now go to the file if init succeeded)
    info!(
        "Logging initialized (or attempted). Log level determined by RUST_LOG or debug_mode (default: {}). Log dir: {}",
        default_level,
        log_dir.display()
    );
    if config.debug_mode {
        debug!("Debug mode enabled via configuration.");
    }

    // 9. Return the guard
    Ok(logger_guard)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config; // Make sure Config is accessible
    use serial_test::serial; // Needed for tests modifying env vars or global state
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::thread;
    use std::time::Duration;
    use tempfile::tempdir;

    // Helper to create a basic Config for testing purposes.
    // Adjust fields based on the actual definition of Config.
    fn create_test_config(debug_mode: bool) -> Config {
        Config {
            search_paths: vec![],
            additional_paths: vec![],
            exclude_patterns: vec![],
            debug_mode,
            direct_selection: None,
            // Add other necessary fields from the actual Config struct if they exist
            // and are required for initialization or logging logic.
            // For example, if config file loading affects defaults:
        }
    }

    // Helper function to get the expected log directory path based on a temporary data home.
    fn get_expected_log_dir(temp_data_home: &PathBuf) -> PathBuf {
        temp_data_home.join(APP_NAME)
    }

    #[test]
    #[serial] // Modifies environment variables
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

        // Call init to trigger directory creation logic
        let config = create_test_config(false);
        let _guard = init(&config).expect("Logger initialization failed");

        // Restore original environment variable
        match original_xdg_data_home {
            Some(val) => unsafe { env::set_var("XDG_DATA_HOME", val) },
            None => unsafe { env::remove_var("XDG_DATA_HOME") },
        }

        // Assert that the log directory was created by init
        assert!(
            expected_log_dir.exists(),
            "Log directory '{}' should have been created by init",
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

        let config = create_test_config(false); // Use info level
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

        // Initialize logging
        let guard = init(&config).expect("Logger initialization failed");

        // Log a message to trigger file write
        tracing::info!("Test message for log file creation.");

        // Drop the guard to ensure logs are flushed
        drop(guard);
        thread::sleep(Duration::from_millis(100)); // Allow time for flush

        // Restore environment variable
        match original_xdg_data_home {
            Some(val) => unsafe { env::set_var("XDG_DATA_HOME", val) },
            None => unsafe { env::remove_var("XDG_DATA_HOME") },
        }

        // Assertions
        assert!(
            expected_log_dir.exists(),
            "Log directory should exist after init"
        );
        assert!(
            expected_log_file.exists(),
            "Log file '{}' should exist after init and logging",
            expected_log_file.display()
        );
        assert!(
            expected_log_file.is_file(),
            "Log file path '{}' should be a file",
            expected_log_file.display()
        );

        // Check if the file has content
        let metadata = fs::metadata(&expected_log_file).expect("Failed to get log file metadata");
        assert!(
            metadata.len() > 0,
            "Log file should not be empty after logging"
        );

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
        let original_xdg_data_home = env::var_os("XDG_DATA_HOME");
        unsafe {
            env::set_var("XDG_DATA_HOME", &temp_data_home_debug);
        }

        let debug_config = create_test_config(true);
        let _guard_debug = init(&debug_config).expect("Logger init failed for debug test");
        // Check the effective level filter (requires accessing subscriber state, which is complex)
        // Instead, we verify the *default* filter string logic within init.
        // The init function logs the default level used if RUST_LOG isn't set.
        // We can indirectly test by checking if DEBUG level logs are emitted.
        tracing::debug!("This debug message should be logged in debug mode.");
        drop(_guard_debug); // Flush logs
        thread::sleep(Duration::from_millis(100)); // Allow time for flush

        let log_file_debug =
            get_expected_log_dir(&temp_data_home_debug).join(format!("{}.log", APP_NAME));
        let content_debug =
            fs::read_to_string(&log_file_debug).expect("Failed to read debug log file");
        assert!(
            content_debug.contains("level determined by RUST_LOG or debug_mode (default: DEBUG)"),
            "Log init message should indicate DEBUG default"
        );
        assert!(
            content_debug.contains("This debug message should be logged"),
            "Debug message missing in debug mode"
        );

        // Test with debug_mode = false
        unsafe {
            env::remove_var("RUST_LOG");
        } // Ensure RUST_LOG is not set
        let temp_dir_info = tempdir().expect("Failed temp dir for info test");
        let temp_data_home_info = temp_dir_info.path().to_path_buf();
        unsafe {
            env::set_var("XDG_DATA_HOME", &temp_data_home_info);
        } // Set again for this part

        let info_config = create_test_config(false);
        let _guard_info = init(&info_config).expect("Logger init failed for info test");
        tracing::debug!("This debug message should NOT be logged in info mode.");
        tracing::info!("This info message should be logged in info mode.");
        drop(_guard_info); // Flush logs
        thread::sleep(Duration::from_millis(100)); // Allow time for flush

        let log_file_info =
            get_expected_log_dir(&temp_data_home_info).join(format!("{}.log", APP_NAME));
        let content_info =
            fs::read_to_string(&log_file_info).expect("Failed to read info log file");
        assert!(
            content_info.contains("level determined by RUST_LOG or debug_mode (default: INFO)"),
            "Log init message should indicate INFO default"
        );
        assert!(
            content_info.contains("This info message should be logged"),
            "Info message missing in info mode"
        );
        assert!(
            !content_info.contains("This debug message should NOT be logged"),
            "Debug message unexpectedly present in info mode"
        );

        // Restore original environment variable
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
