// Handles application configuration, primarily derived from command-line arguments.
//
// This module defines the structure for command-line arguments using `clap`
// and the main `Config` struct that holds the application's runtime settings.

use crate::error::{ConfigError, PathValidationError};
use crate::path_utils::expand_tilde; // For expanding tilde in paths
use clap::Parser;
use regex::Regex;
use serde_derive::Deserialize;
use std::fs;
use std::io;
use std::path::PathBuf;
use tracing::{debug, error, info, trace, warn};

/// Validates a single path to ensure it exists and is a directory.
fn validate_path_is_directory(path: &PathBuf) -> std::result::Result<(), PathValidationError> {
    trace!(path = %path.display(), "Validating path");
    match fs::metadata(path) {
        Ok(metadata) => {
            if metadata.is_dir() {
                trace!(path = %path.display(), "Path is a valid directory");
                Ok(())
            } else {
                warn!(path = %path.display(), "Path exists but is not a directory");
                Err(PathValidationError::NotADirectory { path: path.clone() })
            }
        }
        Err(e) => {
            error!(path = %path.display(), error = %e, "Error accessing path metadata");
            match e.kind() {
                io::ErrorKind::NotFound => {
                    Err(PathValidationError::DoesNotExist { path: path.clone() })
                }
                io::ErrorKind::PermissionDenied => {
                    Err(PathValidationError::PermissionDenied { path: path.clone() })
                }
                _ => Err(PathValidationError::FilesystemError {
                    path: path.clone(),
                    source: e,
                }),
            }
        }
    }
}

/// Loads configuration from the TOML file if present.
///
/// Path: ~/.config/tmux-sessionizer/tmux-sessionizer.toml (platform-dependent)
///
/// Returns `Ok(Some(FileConfig))` if loaded and parsed successfully.
/// Returns `Ok(None)` if the config directory is not found or the file does not exist.
/// Returns `Err(ConfigError)` for IO errors during reading or parsing errors.
fn load_config_file() -> Result<Option<FileConfig>, ConfigError> {
    let Some(config_dir) = dirs::config_dir() else {
        // Use eprintln! because tracing might not be set up yet
        eprintln!("[ERROR] Could not determine the user's config directory.");
        return Err(ConfigError::CannotDetermineConfigDir);
    };

    let mut config_path = config_dir;
    config_path.push("tmux-sessionizer"); // Application-specific subdirectory
    config_path.push("tmux-sessionizer.toml"); // The config file itself

    // Use eprintln! for logging before tracing is initialized
    eprintln!("[DEBUG] Attempting to load configuration from file: {}", config_path.display());

    if !config_path.exists() {
        eprintln!("[INFO] Configuration file not found at {}. Proceeding with defaults and CLI arguments.", config_path.display());
        return Ok(None);
    }

    eprintln!("[INFO] Configuration file found at {}. Reading and parsing.", config_path.display());
    let content = match fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[ERROR] Failed to read configuration file content from {}: {}", config_path.display(), e);
            return Err(ConfigError::FileReadError {
                path: config_path,
                source: e,
            });
        }
    };

    eprintln!("[TRACE] Successfully read configuration file content:\n{}", content);
    match toml::from_str::<FileConfig>(&content) {
        Ok(parsed_config) => {
            eprintln!("[INFO] Successfully parsed configuration file: {}", config_path.display());
            Ok(Some(parsed_config))
        }
        Err(e) => {
            eprintln!("[ERROR] Failed to parse TOML configuration from file {}: {}", config_path.display(), e);
            Err(ConfigError::FileParseError {
                path: config_path,
                source: e,
            })
        }
    }
}

/// Command-line arguments parsed by clap.
#[derive(Parser, Debug)]
#[command(name = "tmux-sessionizer")]
#[command(author, version)]
#[command(
    about = "Scans specified directories, identifies projects (including Git repositories and worktrees), and presents them via a fuzzy finder (like skim) for quick tmux session creation or switching.",
    long_about = r#"
tmux-sessionizer simplifies managing tmux sessions for your projects.

It scans predefined search paths (like ~/.config) and any additional paths provided.
It intelligently detects Git repositories and their linked worktrees, presenting them clearly.

Use the fuzzy finder (skim) to quickly select a project and jump into its tmux session.
If a session for the selected project exists, it attaches to it. Otherwise, it creates a new session.

You can also provide a direct path or project name as an argument to bypass the fuzzy finder.
"#
)]
pub(crate) struct CliArgs {
    /// Enable detailed debug logging.
    #[arg(short, long, action = clap::ArgAction::SetTrue, help = "Enable debug logging to stderr")]
    debug: bool,
    /// Directly select a path or name, skipping the fuzzy finder.
    #[arg(
        index = 1,
        help = "Directly select a project by path or name, skipping the fuzzy finder",
        long_help = "Provide a full path (e.g., /path/to/project) or a project name (e.g., my_project) to directly create or switch to its tmux session without showing the fuzzy finder interface."
    )]
    direct_selection: Option<String>,
    // #[arg(long, value_delimiter = ',', help = "Additional search paths, comma-separated")]
    // additional_paths: Option<Vec<PathBuf>>,
}

/// Represents the structure of the configuration file (e.g., tmux-sessionizer.toml).
/// Used for deserializing the configuration from TOML format.
#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)] // Optional: Error if unknown fields are in the TOML
pub(crate) struct FileConfig {
    /// Optional list of default search paths from the config file.
    #[serde(default)]
    pub search_paths: Option<Vec<String>>,
    /// Optional list of additional search paths from the config file.
    #[serde(default)]
    pub additional_paths: Option<Vec<String>>,
    /// Optional list of patterns to exclude from the search.
    #[serde(default)]
    pub exclude_patterns: Option<Vec<String>>,
}

/// Holds the application's runtime configuration.
#[derive(Debug)]
pub struct Config {
    /// Default directories to search for projects.
    pub search_paths: Vec<PathBuf>,
    /// Additional directories specified by the user to search. (Currently unused CLI arg)
    pub additional_paths: Vec<PathBuf>,
    /// Patterns to exclude directories from the search. (Currently unused CLI arg)
    pub exclude_patterns: Vec<Regex>,
    /// Flag indicating whether debug logging is enabled.
    pub debug_mode: bool,
    /// An optional path or name provided directly by the user, bypassing the fuzzy finder.
    pub direct_selection: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        // The tilde (~) needs to be expanded to the user's home directory.
        // We'll handle tilde expansion when these paths are actually used,
        // or when the config is fully parsed. For Default, we'll store them as is.
        let default_search_paths = vec![PathBuf::from("~/.config")];

        Config {
            search_paths: default_search_paths,
            additional_paths: Vec::new(),
            exclude_patterns: Vec::new(),
            debug_mode: false,
            direct_selection: None,
        }
    }
}

impl Config {
    /// Creates a new `Config` instance by loading from file (if exists),
    /// parsing command-line arguments, and merging them.
    /// Also performs validation.
    pub fn new() -> Result<Self, ConfigError> {
        // Use eprintln! here because tracing is not yet initialized
        eprintln!("[TRACE] Setting up configuration");
        let cli_args = CliArgs::parse();
        eprintln!("[DEBUG] Parsed command line arguments: {:?}", cli_args);

        // Load configuration from file
        let file_config = match load_config_file() {
            Ok(fc) => {
                eprintln!("[TRACE] Configuration file load attempt completed. Loaded: {}", fc.is_some());
                fc
            }
            Err(e) => {
                // Use eprintln! for errors occurring before tracing is set up
                eprintln!("[ERROR] Failed to load or parse the configuration file: {}. This is a fatal configuration error.", e);
                return Err(e);
            }
        };

        let config = Self::build(file_config, cli_args)?;
        eprintln!("[DEBUG] Constructed final configuration: {:?}", config);

        // Validate paths after merging and expansion
        config.validate()?; // Validation logs internally using tracing, but that's okay if it happens after setup

        Ok(config)
    }

    /// Builds the final `Config` by merging defaults, file configuration, and CLI arguments.
    /// Handles path expansion and regex compilation.
    fn build(file_config: Option<FileConfig>, cli_args: CliArgs) -> Result<Self, ConfigError> {
        let defaults = Config::default();
        let mut config = Config {
            // Start with defaults. Note: search_paths from default are PathBufs with tildes.
            search_paths: defaults.search_paths,
            additional_paths: defaults.additional_paths,
            exclude_patterns: defaults.exclude_patterns, // Default is empty Vec<Regex>
            debug_mode: defaults.debug_mode,
            direct_selection: defaults.direct_selection,
        };

        // 1. Apply File Configuration (if present)
        if let Some(fc) = file_config {
            debug!(?fc, "Applying configuration from file");
            if let Some(search_paths_str) = fc.search_paths {
                config.search_paths = search_paths_str.into_iter().map(PathBuf::from).collect();
                trace!(paths = ?config.search_paths, "Overridden search_paths from file config (pre-expansion)");
            }
            if let Some(additional_paths_str) = fc.additional_paths {
                config.additional_paths = additional_paths_str
                    .into_iter()
                    .map(PathBuf::from)
                    .collect();
                trace!(paths = ?config.additional_paths, "Overridden additional_paths from file config (pre-expansion)");
            }
            if let Some(exclude_patterns_str) = fc.exclude_patterns {
                let mut regex_patterns = Vec::new();
                for pattern_str in exclude_patterns_str {
                    match Regex::new(&pattern_str) {
                        Ok(re) => regex_patterns.push(re),
                        Err(e) => {
                            error!(pattern = %pattern_str, error = %e, "Invalid regex pattern in config file");
                            return Err(ConfigError::InvalidRegex {
                                // Using InvalidRegex from src/error.rs
                                pattern: pattern_str,
                                source: e,
                            });
                        }
                    }
                }
                config.exclude_patterns = regex_patterns;
                trace!(
                    count = config.exclude_patterns.len(),
                    "Loaded exclude_patterns from file config"
                );
            }
        } else {
            debug!("No configuration file loaded or found. Using defaults combined with CLI args.");
        }

        // 2. Apply CLI Argument Overrides (Highest Precedence)
        debug!(?cli_args, "Applying CLI arguments");
        if cli_args.debug {
            config.debug_mode = true;
            trace!("Overridden debug_mode from CLI args");
        }
        if cli_args.direct_selection.is_some() {
            config.direct_selection = cli_args.direct_selection;
            trace!(selection = ?config.direct_selection, "Overridden direct_selection from CLI args");
        }
        // TODO: Add CLI overrides for search_paths, additional_paths, exclude_patterns if/when implemented in CliArgs

        // 3. Post-process: Expand Tilde and Normalize Paths for all relevant path collections
        trace!("Expanding tilde and normalizing paths for search_paths and additional_paths");
        config.search_paths = config
            .search_paths
            .into_iter()
            .filter_map(|p| {
                let expanded = expand_tilde(&p);
                if expanded.is_none() && p.starts_with("~") {
                    warn!(path = ?p.display(), "Could not expand tilde for path. It will be omitted.");
                }
                expanded
            })
            .collect();
        trace!(expanded_search_paths = ?config.search_paths, "Search paths after tilde expansion");

        config.additional_paths = config
            .additional_paths
            .into_iter()
            .filter_map(|p| {
                let expanded = expand_tilde(&p);
                if expanded.is_none() && p.starts_with("~") {
                    warn!(path = ?p.display(), "Could not expand tilde for additional path. It will be omitted.");
                }
                expanded
            })
            .collect();
        trace!(expanded_additional_paths = ?config.additional_paths, "Additional paths after tilde expansion");
        // Note: expand_tilde logs errors internally if home dir isn't found.
        // We filter_map to omit paths that couldn't be expanded.

        Ok(config)
    }

    /// Validates the configuration, checking if specified paths exist and are directories.
    /// This should be called *after* paths are expanded and finalized.
    fn validate(&self) -> std::result::Result<(), ConfigError> {
        info!("Validating configuration paths");
        for path in self.search_paths.iter().chain(self.additional_paths.iter()) {
            validate_path_is_directory(path)?; // The ? automatically converts PathValidationError to ConfigError::InvalidPath
        }
        debug!("All configured paths validated successfully.");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
                "Missing path: {:?}",
                path
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
            direct_selection: None,
        };

        let config =
            Config::build(Some(file_config_content), cli_args).expect("Config build failed");

        assert!(!config.debug_mode); // From CLI (or default if CLI didn't set)
        assert_eq!(config.direct_selection, None); // From CLI (or default)

        // Check paths are from file_config and expanded
        let home = get_home_dir_for_test();
        let expected_search_paths = vec![PathBuf::from("/etc/from_file"), home.join("file_dev")];
        assert_eq!(config.search_paths.len(), expected_search_paths.len());
        for path in expected_search_paths {
            assert!(
                config.search_paths.contains(&path),
                "Missing search path: {:?}",
                path
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
            direct_selection: Some("cli_selected_project".to_string()), // CLI overrides default None and file
        };
        // Note: Current CliArgs doesn't have fields for paths/patterns.
        // So, file paths/patterns will take precedence over defaults if present.
        // If CLI args for these are added later, this test would need adjustment.

        let config =
            Config::build(Some(file_config_content), cli_args).expect("Config build failed");

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
            direct_selection: None,
        };

        let result = Config::build(Some(file_config_with_bad_regex), cli_args);
        assert!(result.is_err());
        match result.err().unwrap() {
            ConfigError::InvalidRegex { pattern, .. } => {
                assert_eq!(pattern, "[invalidRegex");
            }
            other_error => panic!("Expected InvalidRegex error, got {:?}", other_error),
        }
    }

    #[test]
    fn test_build_empty_file_config_uses_defaults_and_cli() {
        let empty_file_config = FileConfig::default(); // All fields are Option<Vec<String>>, so default is all None
        let cli_args = CliArgs {
            debug: true,
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
                "Missing default search path: {:?}",
                path
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
                let mut file =
                    File::create(&config_path).expect("Failed to create temp config file");
                if let Some(content) = file_content {
                    write!(file, "{}", content).expect("Failed to write to temp config file");
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
            other => panic!("Expected FileParseError, got {:?}", other),
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
            other => panic!(
                "Expected FileParseError (due to unknown field), got {:?}",
                other
            ),
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
        };

        let result = config.validate();
        assert!(result.is_err());
        match result.err().unwrap() {
            ConfigError::InvalidPath(PathValidationError::DoesNotExist { path }) => {
                assert_eq!(path, non_existent_path);
            }
            other => panic!("Expected DoesNotExist error, got {:?}", other),
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
        };

        let result = config.validate();
        assert!(result.is_err());
        match result.err().unwrap() {
            ConfigError::InvalidPath(PathValidationError::NotADirectory { path }) => {
                assert_eq!(path, file_path);
            }
            other => panic!("Expected NotADirectory error, got {:?}", other),
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
        };

        let result = config.validate();
        assert!(result.is_err()); // Should fail on the first invalid path encountered
        match result.err().unwrap() {
            ConfigError::InvalidPath(PathValidationError::DoesNotExist { path }) => {
                assert_eq!(path, non_existent_path); // Error should be for the invalid path
            }
            other => panic!(
                "Expected DoesNotExist error for the second path, got {:?}",
                other
            ),
        }
    }
}
