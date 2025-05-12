// Handles application configuration, primarily derived from command-line arguments.
//
// This module defines the structure for command-line arguments using `clap`
// and the main `Config` struct that holds the application's runtime settings.

use clap::Parser;
use regex::Regex;
use std::path::PathBuf;
use tracing::debug;

/// Command-line arguments parsed by clap.
#[derive(Parser, Debug)]
#[command(name = "tmux-sessionizer")]
#[command(author, version, about = "A utility for managing tmux sessions based on project directories.", long_about = None)]
pub(crate) struct CliArgs {
    /// Enable detailed debug logging.
    #[arg(short, long, action = clap::ArgAction::SetTrue, help = "Enable debug mode")]
    debug: bool,
    /// Directly select a path or name, skipping the fuzzy finder.
    #[arg(
        index = 1,
        help = "Direct path or name to select. If provided, fuzzy finder is skipped."
    )]
    direct_selection: Option<String>,
    // #[arg(long, value_delimiter = ',', help = "Additional search paths, comma-separated")]
    // additional_paths: Option<Vec<PathBuf>>,
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
