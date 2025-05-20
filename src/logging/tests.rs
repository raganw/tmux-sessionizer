use super::*;
use serial_test::serial;
use std::env;
use std::fs;
use std::path::Path;
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
pub fn init_tracing(
    log_directory: &Path,
    level: &str,
) -> Result<(WorkerGuard, TracingDefaultGuard)> {
    let log_level = Level::from_str(level)
        .map_err(|_| AppError::LoggingConfig(format!("Invalid log level string: {level}")))?;

    let (guard, subscriber, log_dir_path) = init_file_subscriber(log_directory, log_level)?;
    let subscriber_guard = tracing::subscriber::set_default(subscriber);
    // This log message will go to the newly set default subscriber (i.e., the file)
    info!(
        "initialized tracing. Log directory: {}",
        log_dir_path.display()
    );
    Ok((guard, subscriber_guard))
}

#[test]
#[serial]
fn test_xdg_directory_determination_and_creation() {
    let temp_base_dir = tempdir().expect("Failed to create temp base dir for log test");
    // Define the log directory path within the temp base directory
    let log_dir_path = temp_base_dir.path().join("test_logs");

    // Ensure the directory does not exist before the call within init
    // (it shouldn't as log_dir_path is unique to this test run)
    if log_dir_path.exists() {
        fs::remove_dir_all(&log_dir_path).expect("Failed to clean up pre-existing test log dir");
    }

    // Call init_tracing to trigger directory creation logic
    let (_worker_guard, _subscriber_guard) =
        init_tracing(&log_dir_path, "info").expect("Logger initialization failed");

    assert!(
        log_dir_path.exists(),
        "Log directory '{}' should have been created",
        log_dir_path.display()
    );
    assert!(
        log_dir_path.is_dir(),
        "Log directory path '{}' should point to a directory",
        log_dir_path.display()
    );

    // Clean up the created directory
    // temp_base_dir will be cleaned up automatically when it goes out of scope
}

#[test]
#[serial] // Modifies environment variables and global tracing state
fn test_log_file_creation() {
    let temp_base_dir = tempdir().expect("Failed to create temp base dir for log file test");
    // Define the log directory path within the temp base directory
    let log_dir_path = temp_base_dir.path().join("test_log_files");

    println!("Test log directory: {}", log_dir_path.display());

    // Ensure clean state
    if log_dir_path.exists() {
        println!(
            "Cleaning up pre-existing log directory: {}",
            log_dir_path.display()
        );
        fs::remove_dir_all(&log_dir_path).expect("Failed to clean up pre-existing test log dir");
    }
    // create the directory
    // fs::create_dir_all(&log_dir_path).expect("Failed to create test log dir"); // init_tracing will do this
    // create the file
    // fs::File::create(&expected_log_file).expect("Failed to create test log file"); // Not needed, appender creates it
    // write debug message to the file
    // fs::write(
    //     &expected_log_file,
    //     "This is a test log file for tmux-sessionizer.",
    // )
    // .expect("Failed to write to test log file"); // Not needed

    // Initialize logging using init_tracing
    let (worker_guard, subscriber_guard) =
        init_tracing(&log_dir_path, "info").expect("Logger initialization failed");

    tracing::info!("Test message for log file creation.");

    drop(subscriber_guard);
    drop(worker_guard);
    thread::sleep(Duration::from_millis(100)); // Give a bit more time for flush

    // Find the created log file
    let entries: Vec<fs::DirEntry> = fs::read_dir(&log_dir_path)
        .expect("Failed to read log directory")
        .filter_map(Result::ok)
        .collect();

    assert_eq!(
        entries.len(),
        1,
        "Expected a single log file in the directory. Found: {:?}",
        entries.iter().map(|e| e.path()).collect::<Vec<_>>()
    );
    let log_file_path = entries[0].path();
    assert!(
        log_file_path
            .file_name()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default()
            .starts_with(APP_NAME),
        "Log file name should start with APP_NAME"
    );
    assert!(
        log_file_path
            .file_name()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default()
            .ends_with(".log"),
        "Log file name should end with .log"
    );
    println!("Actual log file path: {}", log_file_path.display());

    // read the contents of the temp directory
    let temp_dir_content = fs::read_dir(&log_dir_path)
        .expect("Failed to read temp directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    println!(
        "Temp directory content: {}",
        temp_dir_content
            .iter()
            .map(|path| format!("  {}", path.display()))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // read the log file to check if the message was logged
    let log_file_content =
        fs::read_to_string(&log_file_path).expect("Failed to read log file content");
    println!(
        "Log file content: {}",
        log_file_content
            .lines()
            .map(|line| format!("  {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    assert!(
        log_dir_path.exists(),
        "Log directory '{}' should exist after init",
        log_dir_path.display()
    );
    assert!(
        log_file_path.exists(),
        "Log file '{}' should exist after init and logging. Content: {:?}",
        log_file_path.display(),
        fs::read_to_string(&log_file_path).unwrap_or_else(|_| "Error reading file".to_string())
    );
    assert!(
        log_file_path.is_file(),
        "Log file path '{}' should be a file",
        log_file_path.display()
    );

    let metadata = fs::metadata(&log_file_path).expect("Failed to get log file metadata");
    assert!(
        metadata.len() > 0,
        "Log file should not be empty after logging"
    );
    let content = fs::read_to_string(&log_file_path).unwrap();
    assert!(content.contains("Test message for log file creation."));
    assert!(content.contains("initialized tracing. Log directory:"));

    // Clean up
    // temp_base_dir will be cleaned up automatically
}

#[test]
#[serial] // Modifies environment variables and global tracing state
fn test_debug_mode_level_setting() {
    // Test with debug_mode = true
    unsafe {
        env::remove_var("RUST_LOG");
    } // Ensure RUST_LOG is not set to interfere
    let temp_base_dir_debug = tempdir().expect("Failed temp dir for debug test");
    let log_dir_debug = temp_base_dir_debug.path().join("debug_logs");

    let (_w_guard_debug, s_guard_debug) =
        init_tracing(&log_dir_debug, "debug").expect("Logger init failed for debug test");
    tracing::debug!("This debug message should be logged in debug mode.");
    drop(s_guard_debug);
    // _w_guard_debug is implicitly dropped here
    thread::sleep(Duration::from_millis(100));

    let debug_entries: Vec<fs::DirEntry> = fs::read_dir(&log_dir_debug)
        .expect("Failed to read debug log directory")
        .filter_map(Result::ok)
        .collect();
    assert_eq!(
        debug_entries.len(),
        1,
        "Expected one log file in debug_logs. Found: {:?}",
        debug_entries.iter().map(|e| e.path()).collect::<Vec<_>>()
    );
    let log_file_debug = debug_entries[0].path();
    assert!(
        log_file_debug
            .file_name()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default()
            .starts_with(APP_NAME)
    );
    assert!(
        log_file_debug
            .file_name()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default()
            .ends_with(".log")
    );

    let content_debug = fs::read_to_string(&log_file_debug).expect("Failed to read debug log file");
    assert!(
        content_debug.contains(&format!(
            "initialized tracing. Log directory: {}",
            log_dir_debug.display()
        )),
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
    let temp_base_dir_info = tempdir().expect("Failed temp dir for info test");
    let log_dir_info = temp_base_dir_info.path().join("info_logs");

    let (_w_guard_info, s_guard_info) =
        init_tracing(&log_dir_info, "info").expect("Logger init failed for info test");
    tracing::debug!("This debug message should NOT be logged in info mode.");
    tracing::info!("This info message should be logged in info mode.");
    drop(s_guard_info);
    // _w_guard_info is implicitly dropped here
    thread::sleep(Duration::from_millis(100));

    let info_entries: Vec<fs::DirEntry> = fs::read_dir(&log_dir_info)
        .expect("Failed to read info log directory")
        .filter_map(Result::ok)
        .collect();
    assert_eq!(
        info_entries.len(),
        1,
        "Expected one log file in info_logs. Found: {:?}",
        info_entries.iter().map(|e| e.path()).collect::<Vec<_>>()
    );
    let log_file_info = info_entries[0].path();
    assert!(
        log_file_info
            .file_name()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default()
            .starts_with(APP_NAME)
    );
    assert!(
        log_file_info
            .file_name()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default()
            .ends_with(".log")
    );

    let content_info = fs::read_to_string(&log_file_info).expect("Failed to read info log file");
    assert!(
        content_info.contains(&format!(
            "initialized tracing. Log directory: {}",
            log_dir_info.display()
        )),
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

    // Clean up
    // temp_base_dir_debug and temp_base_dir_info will be cleaned up automatically
}

// Note on Rotation Testing:
// Testing the actual file rotation (e.g., keeping only 2 files after several days)
// is complex and brittle in unit tests. It would require manipulating time or
// relying heavily on the internal implementation details of `tracing-appender`.
// We trust that configuring `Rotation::DAILY` correctly instructs the library
// to perform daily rotation. These tests focus on verifying the initial setup,
// directory/file creation, and log level configuration based on `debug_mode`.
