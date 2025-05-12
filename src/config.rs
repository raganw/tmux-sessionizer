// Handles application configuration, primarily derived from command-line arguments.
//
// This module defines the structure for command-line arguments using `clap`
// and the main `Config` struct that holds the application's runtime settings.

use crate::error::{ConfigError, PathValidationError};
use clap::Parser;
use regex::Regex;
use serde::Deserialize;
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

/// Command-line arguments parsed by clap.
#[derive(Parser, Debug)]
#[command(name = "tmux-sessionizer")]
#[command(author, version)]
#[command(
    about = "Scans specified directories, identifies projects (including Git repositories and worktrees), and presents them via a fuzzy finder (like skim) for quick tmux session creation or switching.",
    long_about = r#"
tmux-sessionizer simplifies managing tmux sessions for your projects.

It scans predefined search paths (like ~/Development) and any additional paths provided.
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
        let default_search_paths = vec![
            PathBuf::from("~/Development"),
            PathBuf::from("~/Development/raganw"),
            PathBuf::from("~/.config"),
        ];

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
    /// Creates a new `Config` instance by parsing command-line arguments.
    pub fn new() -> Self {
        let cli_args = CliArgs::parse();
        debug!(parsed_cli_args = ?cli_args, "Parsed command line arguments");
        Self::from_args(cli_args)
    }

    /// Creates a `Config` instance from pre-parsed `CliArgs`. Useful for testing.
    pub fn from_args(args: CliArgs) -> Self {
        let mut default_config = Config::default();

        // Override default debug_mode if --debug flag is present
        if args.debug {
            default_config.debug_mode = true;
        }

        // Set direct_selection from CLI args
        default_config.direct_selection = args.direct_selection;

        default_config
    }

    /// Validates the configuration, checking if specified paths exist and are directories.
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

    #[test]
    fn test_default_config() {
        let config = Config::default();

        assert!(!config.debug_mode);
        assert_eq!(config.direct_selection, None);
        assert_eq!(
            config.search_paths,
            vec![
                PathBuf::from("~/Development"),
                PathBuf::from("~/Development/raganw"),
                PathBuf::from("~/.config"),
            ]
        );
        assert!(config.additional_paths.is_empty());
        assert!(config.exclude_patterns.is_empty());
    }

    #[test]
    fn test_config_from_args_no_special_args() {
        // The first element is traditionally the program name
        let cli_args = CliArgs::try_parse_from(["tmux-sessionizer"]).unwrap();
        let config = Config::from_args(cli_args);

        assert!(!config.debug_mode);
        assert_eq!(config.direct_selection, None);
        assert_eq!(config.search_paths, Config::default().search_paths);
    }

    #[test]
    fn test_config_from_args_with_debug() {
        let cli_args = CliArgs::try_parse_from(["tmux-sessionizer", "--debug"]).unwrap();
        let config = Config::from_args(cli_args);

        assert!(config.debug_mode);
        assert_eq!(config.direct_selection, None);
    }

    #[test]
    fn test_config_from_args_with_direct_selection() {
        let project_name = "my_project";
        let cli_args = CliArgs::try_parse_from(["tmux-sessionizer", project_name]).unwrap();
        let config = Config::from_args(cli_args);

        assert!(!config.debug_mode);
        assert_eq!(config.direct_selection, Some(project_name.to_string()));
    }

    #[test]
    fn test_config_from_args_with_debug_and_direct_selection() {
        let project_name = "another_project";
        let cli_args =
            CliArgs::try_parse_from(["tmux-sessionizer", "--debug", project_name]).unwrap();
        let config = Config::from_args(cli_args);

        assert!(config.debug_mode);
        assert_eq!(config.direct_selection, Some(project_name.to_string()));
    }
}
