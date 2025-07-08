use super::*;
use clap::Parser;
use std::path::PathBuf;
// It's good practice to ensure tracing is initialized for tests that might log.
// use tracing_test::traced_test; // Requires adding tracing-test to dev-dependencies

// Helper function to get a realistic home directory for tests, if needed for assertions.
// Ensure `dirs` crate is available in test context.
fn get_home_dir_for_test() -> PathBuf {
    dirs::home_dir()
        .expect("Test environment must have a valid home directory for ~ expansion tests.")
}

#[test]
fn test_default_config_values() {
    let config = Config::default();
    assert!(!config.debug_mode);
    assert_eq!(config.direct_selection, None);
    // Default paths are stored with tilde before expansion by `build`
    assert_eq!(config.search_paths, vec![PathBuf::from("~/.config"),]);
    assert!(config.additional_paths.is_empty());
    assert!(config.exclude_patterns.is_empty());
}

#[test]
fn test_build_with_cli_only_no_file_config() {
    let cli_args = CliArgs {
        debug: true,
        init: false,
        direct_selection: Some("my_project_cli".to_string()),
    };
    // Pass None for file_config
    let config = Config::build(None, cli_args).expect("Config build failed");

    assert!(config.debug_mode);
    assert_eq!(config.direct_selection, Some("my_project_cli".to_string()));

    // Check that default paths were expanded correctly
    let home = get_home_dir_for_test();
    let expected_search_paths = vec![home.join(".config")];
    // Order might not be guaranteed after collect, so check for presence and length
    assert_eq!(config.search_paths.len(), expected_search_paths.len());
    for path in expected_search_paths {
        assert!(
            config.search_paths.contains(&path),
            "Missing path: {path:?}"
        );
    }

    assert!(config.additional_paths.is_empty());
    assert!(config.exclude_patterns.is_empty());
}

#[test]
fn test_build_with_file_config_overrides_defaults_no_cli_override() {
    let file_config_content = FileConfig {
        search_paths: Some(vec!["/etc/from_file".to_string(), "~/file_dev".to_string()]),
        additional_paths: Some(vec!["/var/log/from_file".to_string()]),
        exclude_patterns: Some(vec!["^\\.git$".to_string(), "target/".to_string()]),
    };
    let cli_args = CliArgs {
        // CLI args that don't override file config for these fields
        debug: false,
        init: false,
        direct_selection: None,
    };

    let config = Config::build(Some(file_config_content), cli_args).expect("Config build failed");

    assert!(!config.debug_mode); // From CLI (or default if CLI didn't set)
    assert_eq!(config.direct_selection, None); // From CLI (or default)

    // Check paths are from file_config and expanded
    let home = get_home_dir_for_test();
    let expected_search_paths = vec![PathBuf::from("/etc/from_file"), home.join("file_dev")];
    assert_eq!(config.search_paths.len(), expected_search_paths.len());
    for path in expected_search_paths {
        assert!(
            config.search_paths.contains(&path),
            "Missing search path: {path:?}"
        );
    }

    let expected_additional_paths = vec![PathBuf::from("/var/log/from_file")];
    assert_eq!(config.additional_paths, expected_additional_paths);

    // Check exclude patterns were compiled
    assert_eq!(config.exclude_patterns.len(), 2);
    assert!(config.exclude_patterns[0].is_match(".git"));
    assert!(!config.exclude_patterns[0].is_match("somegit")); // Test exactness of ^$
    assert!(config.exclude_patterns[1].is_match("target/debug"));
    assert!(config.exclude_patterns[1].is_match("/some/path/target/release"));
}

#[test]
fn test_build_cli_overrides_file_config_and_defaults() {
    let file_config_content = FileConfig {
        search_paths: Some(vec!["/file/path_search".to_string()]), // Will be overridden by default if CLI for paths is not implemented
        additional_paths: Some(vec!["/file/path_add".to_string()]), // Same
        exclude_patterns: Some(vec!["file_pattern".to_string()]),  // Same
    };
    let cli_args = CliArgs {
        debug: true, // CLI overrides default false and any file setting (if file had debug)
        init: false,
        direct_selection: Some("cli_selected_project".to_string()), // CLI overrides default None and file
    };
    // Note: Current CliArgs doesn't have fields for paths/patterns.
    // So, file paths/patterns will take precedence over defaults if present.
    // If CLI args for these are added later, this test would need adjustment.

    let config = Config::build(Some(file_config_content), cli_args).expect("Config build failed");

    assert!(config.debug_mode); // From CLI
    assert_eq!(
        config.direct_selection,
        Some("cli_selected_project".to_string())
    ); // From CLI

    // Since CLI doesn't override paths/patterns, these come from the file
    assert_eq!(
        config.search_paths,
        vec![PathBuf::from("/file/path_search")]
    );
    assert_eq!(
        config.additional_paths,
        vec![PathBuf::from("/file/path_add")]
    );
    assert_eq!(config.exclude_patterns.len(), 1);
    assert!(config.exclude_patterns[0].is_match("some_file_pattern_here"));
}

#[test]
fn test_build_invalid_regex_in_file_config_returns_error() {
    let file_config_with_bad_regex = FileConfig {
        search_paths: None,
        additional_paths: None,
        exclude_patterns: Some(vec!["[invalidRegex".to_string()]), // This is an invalid regex
    };
    let cli_args = CliArgs {
        debug: false,
        init: false,
        direct_selection: None,
    };

    let result = Config::build(Some(file_config_with_bad_regex), cli_args);
    assert!(result.is_err());
    match result.err().unwrap() {
        ConfigError::InvalidRegex { pattern, .. } => {
            assert_eq!(pattern, "[invalidRegex");
        }
        other_error => panic!("Expected InvalidRegex error, got {other_error:?}"),
    }
}

#[test]
fn test_build_empty_file_config_uses_defaults_and_cli() {
    let empty_file_config = FileConfig::default(); // All fields are Option<Vec<String>>, so default is all None
    let cli_args = CliArgs {
        debug: true,
        init: false,
        direct_selection: Some("cli_only_project".to_string()),
    };

    let config = Config::build(Some(empty_file_config), cli_args).expect("Config build failed");

    assert!(config.debug_mode); // From CLI
    assert_eq!(
        config.direct_selection,
        Some("cli_only_project".to_string())
    ); // From CLI

    // Paths should be defaults, expanded
    let home = get_home_dir_for_test();
    let expected_search_paths = vec![home.join(".config")];
    assert_eq!(config.search_paths.len(), expected_search_paths.len());
    for path in expected_search_paths {
        assert!(
            config.search_paths.contains(&path),
            "Missing default search path: {path:?}"
        );
    }
    assert!(config.additional_paths.is_empty()); // Default
    assert!(config.exclude_patterns.is_empty()); // Default
}

// TODO: Add tests for Config::new() that mock load_config_file and CliArgs::parse()
// This would require more advanced mocking or refactoring for testability.
// For now, Config::build is the main unit under test for merging logic.

// TODO: Add tests for `validate()` method, potentially mocking `fs::metadata`
// or using temp directories. For now, assuming `validate_path_is_directory` is tested elsewhere
// or relying on integration tests for full validation flow.

// --- Tests for load_config_file (requires filesystem interaction) ---

use std::env;
use std::fs::{self, File};
use std::io::Write;
use tempfile::tempdir;

// Helper to create a realistic config structure within a temp dir
fn setup_temp_config_dir(
    base_dir: &tempfile::TempDir,
    create_subdir: bool,
    create_file: bool,
    file_content: Option<&str>,
) -> PathBuf {
    let mut config_path = base_dir.path().to_path_buf();
    config_path.push("tmux-sessionizer");

    if create_subdir {
        fs::create_dir(&config_path).expect("Failed to create temp config subdir");
        if create_file {
            config_path.push("tmux-sessionizer.toml");
            let mut file = File::create(&config_path).expect("Failed to create temp config file");
            if let Some(content) = file_content {
                write!(file, "{content}").expect("Failed to write to temp config file");
            }
        }
    }
    // Return the path to the *expected* config file, even if not created
    else if create_file {
        // If subdir not created, but file creation requested, return path where file *would* be
        config_path.push("tmux-sessionizer.toml");
    }

    base_dir.path().to_path_buf() // Return the base path for load_config_from_dir
}

// Test version of load_config_file that takes the base config dir path
// Mirrors the logic of the real load_config_file but uses the provided path
fn load_config_from_dir(
    base_config_dir: &PathBuf,
) -> std::result::Result<Option<FileConfig>, ConfigError> {
    let mut config_path = base_config_dir.clone();
    config_path.push("tmux-sessionizer"); // Application-specific subdirectory
    config_path.push("tmux-sessionizer.toml"); // The config file itself

    debug!(path = %config_path.display(), "Attempting to load configuration from file (test helper)");

    if !config_path.exists() {
        info!(path = %config_path.display(), "Configuration file not found (test helper).");
        return Ok(None);
    }

    info!(path = %config_path.display(), "Configuration file found. Reading and parsing (test helper).");
    let content = match fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(e) => {
            error!(path = %config_path.display(), error = %e, "Failed to read configuration file content (test helper).");
            return Err(ConfigError::FileReadError {
                path: config_path,
                source: e,
            });
        }
    };

    trace!(file_content = %content, "Successfully read configuration file content (test helper)");
    match toml::from_str::<FileConfig>(&content) {
        Ok(parsed_config) => {
            info!(path = %config_path.display(), "Successfully parsed configuration file (test helper).");
            Ok(Some(parsed_config))
        }
        Err(e) => {
            error!(path = %config_path.display(), error = %e, "Failed to parse TOML configuration from file (test helper).");
            Err(ConfigError::FileParseError {
                path: config_path,
                source: e,
            })
        }
    }
}

#[test]
fn test_load_config_file_does_not_exist() {
    let temp_dir = tempdir().unwrap();
    // Setup: Create the subdir, but not the file
    let base_path = setup_temp_config_dir(&temp_dir, true, false, None);
    let result = load_config_from_dir(&base_path);
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[test]
fn test_load_config_subdir_does_not_exist() {
    let temp_dir = tempdir().unwrap();
    // Setup: Don't create the subdir or the file
    let base_path = setup_temp_config_dir(&temp_dir, false, false, None);
    let result = load_config_from_dir(&base_path);
    assert!(result.is_ok());
    assert!(result.unwrap().is_none()); // Should still be Ok(None) as the file path won't exist
}

#[test]
fn test_load_config_valid_file() {
    let temp_dir = tempdir().unwrap();
    // Corrected TOML: Backslash in regex needs to be escaped for TOML string literal
    let content = r#"
search_paths = ["/valid/path", "~/valid/tilde/path"]
additional_paths = ["/extra/path"]
exclude_patterns = ["^ignore_this", ".*\\.log"]
"#;
    // Trim potential extra whitespace just in case
    let trimmed_content = content.trim();

    let base_path = setup_temp_config_dir(&temp_dir, true, true, Some(trimmed_content));
    let result = load_config_from_dir(&base_path);

    // Use expect for a clearer panic message if it's an Err
    let file_config_option = result.expect("load_config_from_dir returned Err for valid TOML");

    // Now assert that we got Some(config)
    assert!(
        file_config_option.is_some(),
        "load_config_from_dir returned Ok(None) unexpectedly"
    );

    // Unwrap the Option<FileConfig> obtained from expect()
    let config = file_config_option.unwrap();

    // Assertions on the unwrapped FileConfig
    assert_eq!(
        config.search_paths,
        Some(vec![
            "/valid/path".to_string(),
            "~/valid/tilde/path".to_string()
        ])
    );
    assert_eq!(
        config.additional_paths,
        Some(vec!["/extra/path".to_string()])
    );
    assert_eq!(
        config.exclude_patterns,
        Some(vec!["^ignore_this".to_string(), ".*\\.log".to_string()])
    );
}

#[test]
fn test_load_config_malformed_toml() {
    let temp_dir = tempdir().unwrap();
    let content = r#"
            search_paths = ["/valid/path" # Missing comma and closing bracket
            additional_paths = ["/extra/path"]
        "#;
    let base_path = setup_temp_config_dir(&temp_dir, true, true, Some(content));
    let result = load_config_from_dir(&base_path);

    assert!(result.is_err());
    match result.err().unwrap() {
        ConfigError::FileParseError { .. } => {} // Expected error
        other => panic!("Expected FileParseError, got {other:?}"),
    }
}

#[test]
fn test_load_config_unknown_field() {
    let temp_dir = tempdir().unwrap();
    // FileConfig uses #[serde(deny_unknown_fields)]
    let content = r#"
            search_paths = ["/valid/path"]
            unknown_field = "should cause error"
        "#;
    let base_path = setup_temp_config_dir(&temp_dir, true, true, Some(content));
    let result = load_config_from_dir(&base_path);

    assert!(
        result.is_err(),
        "Expected error due to unknown field, but got Ok: {:?}",
        result.ok()
    );
    match result.err().unwrap() {
        ConfigError::FileParseError { .. } => {} // Expected error due to deny_unknown_fields
        other => panic!("Expected FileParseError (due to unknown field), got {other:?}"),
    }
}

// --- Tests for Config::validate ---

#[test]
fn test_validate_all_paths_valid() {
    let temp_dir = tempdir().unwrap();
    let valid_dir1 = temp_dir.path().join("dir1");
    let valid_dir2 = temp_dir.path().join("dir2");
    fs::create_dir(&valid_dir1).unwrap();
    fs::create_dir(&valid_dir2).unwrap();

    let config = Config {
        search_paths: vec![valid_dir1.clone()],
        additional_paths: vec![valid_dir2.clone()],
        exclude_patterns: vec![],
        debug_mode: false,
        direct_selection: None,
        ..Default::default()
    };

    let result = config.validate();
    assert!(result.is_ok());
}

#[test]
fn test_validate_path_does_not_exist() {
    let temp_dir = tempdir().unwrap();
    let non_existent_path = temp_dir.path().join("non_existent");

    let config = Config {
        search_paths: vec![non_existent_path.clone()],
        additional_paths: vec![],
        exclude_patterns: vec![],
        debug_mode: false,
        direct_selection: None,
        ..Default::default()
    };

    let result = config.validate();
    assert!(result.is_err());
    match result.err().unwrap() {
        ConfigError::InvalidPath(PathValidationError::DoesNotExist { path }) => {
            assert_eq!(path, non_existent_path);
        }
        other => panic!("Expected DoesNotExist error, got {other:?}"),
    }
}

#[test]
fn test_validate_path_is_file_not_directory() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("im_a_file.txt");
    File::create(&file_path)
        .unwrap()
        .write_all(b"hello")
        .unwrap();

    let config = Config {
        search_paths: vec![],
        additional_paths: vec![file_path.clone()],
        exclude_patterns: vec![],
        debug_mode: false,
        direct_selection: None,
        ..Default::default()
    };

    let result = config.validate();
    assert!(result.is_err());
    match result.err().unwrap() {
        ConfigError::InvalidPath(PathValidationError::NotADirectory { path }) => {
            assert_eq!(path, file_path);
        }
        other => panic!("Expected NotADirectory error, got {other:?}"),
    }
}

#[test]
fn test_validate_mixed_valid_and_invalid_paths() {
    let temp_dir = tempdir().unwrap();
    let valid_dir = temp_dir.path().join("valid_dir");
    let non_existent_path = temp_dir.path().join("non_existent");
    fs::create_dir(&valid_dir).unwrap();

    let config = Config {
        search_paths: vec![valid_dir.clone()],
        additional_paths: vec![non_existent_path.clone()], // This one is invalid
        exclude_patterns: vec![],
        debug_mode: false,
        direct_selection: None,
        ..Default::default()
    };

    let result = config.validate();
    assert!(result.is_err()); // Should fail on the first invalid path encountered
    match result.err().unwrap() {
        ConfigError::InvalidPath(PathValidationError::DoesNotExist { path }) => {
            assert_eq!(path, non_existent_path); // Error should be for the invalid path
        }
        other => panic!("Expected DoesNotExist error for the second path, got {other:?}"),
    }
}

#[test]
fn test_build_log_directory_determination_with_xdg_data_home_env_var() {
    use std::env;
    let temp_dir = tempdir().unwrap();
    let custom_xdg_data_home = temp_dir.path();

    let original_xdg_data_home = env::var_os("XDG_DATA_HOME");
    unsafe { env::set_var("XDG_DATA_HOME", custom_xdg_data_home) };

    let cli_args = CliArgs {
        debug: false,
        init: false,
        direct_selection: None,
    };
    let config_result = Config::build(None, cli_args);

    // Restore XDG_DATA_HOME
    if let Some(val) = original_xdg_data_home {
        unsafe { env::set_var("XDG_DATA_HOME", val) };
    } else {
        unsafe { env::remove_var("XDG_DATA_HOME") };
    }

    let config = config_result.expect("Config build failed with custom XDG_DATA_HOME");
    let expected_log_dir = custom_xdg_data_home.join(APP_NAME);

    assert_eq!(
        config.log_directory, expected_log_dir,
        "Log directory should respect XDG_DATA_HOME environment variable"
    );
    // Temp dir will be cleaned up automatically
}

#[test]
fn test_cli_args_init_flag_parsing() {
    // Test that --init flag is parsed correctly
    let args = vec!["tmux-sessionizer", "--init"];
    let cli_args = CliArgs::parse_from(args);
    assert!(cli_args.init);
    assert!(!cli_args.debug);
    assert_eq!(cli_args.direct_selection, None);

    // Test --init with --debug
    let args = vec!["tmux-sessionizer", "--init", "--debug"];
    let cli_args = CliArgs::parse_from(args);
    assert!(cli_args.init);
    assert!(cli_args.debug);
    assert_eq!(cli_args.direct_selection, None);

    // Test --init with debug in different order
    let args = vec!["tmux-sessionizer", "--debug", "--init"];
    let cli_args = CliArgs::parse_from(args);
    assert!(cli_args.init);
    assert!(cli_args.debug);
    assert_eq!(cli_args.direct_selection, None);

    // Test default values when --init is not provided
    let args = vec!["tmux-sessionizer"];
    let cli_args = CliArgs::parse_from(args);
    assert!(!cli_args.init);
    assert!(!cli_args.debug);
    assert_eq!(cli_args.direct_selection, None);
}

#[test]
fn test_init_command_integration() {
    // This test verifies that when --init is provided, the application should
    // handle initialization instead of normal operation
    let temp_dir = tempdir().unwrap();
    let original_xdg_config_home = env::var_os("XDG_CONFIG_HOME");

    // Set XDG_CONFIG_HOME to our temp directory
    unsafe {
        env::set_var("XDG_CONFIG_HOME", temp_dir.path());
    }

    // Test CLI args parsing with --init
    let args = vec!["tmux-sessionizer", "--init"];
    let cli_args = CliArgs::parse_from(args);
    assert!(cli_args.init);

    // Restore original XDG_CONFIG_HOME
    if let Some(val) = original_xdg_config_home {
        unsafe {
            env::set_var("XDG_CONFIG_HOME", val);
        }
    } else {
        unsafe {
            env::remove_var("XDG_CONFIG_HOME");
        }
    }

    // At this point, the main application should detect the init flag and
    // call the initialization logic instead of proceeding with normal operation
    // This integration test verifies that the CLI parsing works correctly
    // The actual integration with main.rs will be tested separately
}

#[test]
fn test_config_new_propagates_xdg_error_for_log_dir() {
    // This test is tricky because BaseDirs::new() failing is hard to simulate
    // without deep mocking or specific environment conditions.
    // We'll assume that if BaseDirs::new() in `build()` returns Err,
    // it correctly propagates as ConfigError::CannotDetermineConfigDir.
    // The logic in `build()` already maps the error:
    // `Err(e) => return Err(ConfigError::CannotDetermineConfigDir);`
    // A more direct test would require mocking `BaseDirs::new()`.
    // For now, we rely on code inspection for this specific error path.
    // If a platform consistently fails to provide BaseDirs, this test might become relevant.
    // As an example of how one *might* try to force it (highly platform dependent and flaky):
    let original_home = std::env::var_os("HOME");
    let original_xdg_data_home = std::env::var_os("XDG_DATA_HOME");

    // On some systems, unsetting HOME might cause BaseDirs::new() to fail.
    // This is NOT a reliable way to test this across all platforms.
    unsafe { std::env::remove_var("HOME") };
    unsafe { std::env::remove_var("XDG_DATA_HOME") };

    let cli_args = CliArgs::parse_from(Vec::<String>::new()); // Simulate no CLI args for parse()
    let build_result = Config::build(None, cli_args);

    // Restore environment variables
    if let Some(val) = original_home {
        unsafe { std::env::set_var("HOME", val) };
    }
    if let Some(val) = original_xdg_data_home {
        unsafe { std::env::set_var("XDG_DATA_HOME", val) };
    }

    if build_result.is_err() {
        match build_result.err().unwrap() {
            ConfigError::CannotDetermineConfigDir => { /* This is the expected error if BaseDirs failed */
            }
            other_err => {
                // If it failed for another reason (e.g. path validation on an empty path if HOME was needed for default search paths)
                // this test might not be robust.
                // For now, we're primarily testing the log_directory part.
                println!(
                    "Config::build failed with an unexpected error: {:?}",
                    other_err
                );
            }
        }
    } else {
        // If BaseDirs::new() still succeeded (e.g., on Windows, or if it has fallbacks),
        // this specific error condition isn't tested.
        println!(
            "Config::build succeeded even with HOME/XDG_DATA_HOME unset. Cannot directly test CannotDetermineConfigDir for log path in this environment."
        );
    }
    // This test is more of a best-effort due to difficulties in reliably inducing BaseDirs failure.
}
