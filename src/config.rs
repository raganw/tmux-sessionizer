// Handles application configuration, primarily derived from command-line arguments.
//
// This module defines the structure for command-line arguments using `clap`
// and the main `Config` struct that holds the application's runtime settings.

use crate::error::{ConfigError, PathValidationError};
use crate::path_utils::expand_tilde;
use clap::Parser;
use cross_xdg::BaseDirs;
use regex::Regex;

const APP_NAME: &str = "tmux-sessionizer";
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
    // Use cross_xdg to find the config directory
    let Ok(base_dirs) = BaseDirs::new() else {
        // Re-use the existing error variant, the log provides the detail.
        return Err(ConfigError::CannotDetermineConfigDir);
    };
    // Convert the &Path from config_home() into an owned PathBuf
    let mut config_path: PathBuf = base_dirs.config_home().to_path_buf();

    // Now we can push onto the PathBuf
    config_path.push("tmux-sessionizer"); // Application-specific subdirectory
    config_path.push("tmux-sessionizer.toml"); // The config file itself

    if !config_path.exists() {
        return Ok(None);
    }

    let content = match fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(e) => {
            // Clone the PathBuf for the error variant
            return Err(ConfigError::FileReadError {
                path: config_path.clone(),
                source: e,
            });
        }
    };

    match toml::from_str::<FileConfig>(&content) {
        Ok(parsed_config) => Ok(Some(parsed_config)),
        Err(e) => {
            // Clone the PathBuf for the error variant
            Err(ConfigError::FileParseError {
                path: config_path.clone(),
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
    /// Directory where log files will be stored.
    pub log_directory: PathBuf,
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
            log_directory: PathBuf::new(), // Initialize, will be properly set in `build`
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
        let cli_args = CliArgs::parse();

        // Load configuration from file
        let file_config = match load_config_file() {
            Ok(fc) => fc,
            Err(e) => {
                return Err(e);
            }
        };

        let config = Self::build(file_config, cli_args)?;

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
            log_directory: defaults.log_directory, // This will be set later
        };

        // Determine log directory path (early, before other processing that might log)
        // This uses the APP_NAME constant defined in this file.
        let Ok(xdg_base_dirs) = BaseDirs::new() else {
            // This error indicates a fundamental problem finding user directories.
            return Err(ConfigError::CannotDetermineConfigDir);
        };
        config.log_directory = xdg_base_dirs.data_home().join(APP_NAME);
        trace!(log_dir = %config.log_directory.display(), "Determined log directory path");

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
mod tests;
