use clap::Parser;
use regex::Regex;
use std::path::PathBuf;
use tracing::debug;

#[derive(Parser, Debug)]
#[command(name = "tmux-sessionizer")]
#[command(author, version, about = "A utility for managing tmux sessions based on project directories.", long_about = None)]
// Make CliArgs pub(crate)
pub(crate) struct CliArgs {
    #[arg(short, long, action = clap::ArgAction::SetTrue, help = "Enable debug mode")]
    debug: bool,
    // Placeholder for potential future arguments for paths or exclusions
    // For example:
    #[arg(
        index = 1,
        help = "Direct path or name to select. If provided, fuzzy finder is skipped."
    )]
    direct_selection: Option<String>,
    // #[arg(long, value_delimiter = ',', help = "Additional search paths, comma-separated")]
    // additional_paths: Option<Vec<PathBuf>>,
}

#[derive(Debug)] // Added Debug derive for easier inspection later
pub struct Config {
    pub search_paths: Vec<PathBuf>,
    pub additional_paths: Vec<PathBuf>,
    pub exclude_patterns: Vec<Regex>,
    pub debug_mode: bool,
    pub direct_selection: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        // Default search paths. These might need adjustment to match the original script's behavior.
        // For now, let's use common development directories.
        // The tilde (~) needs to be expanded to the user's home directory.
        // We'll handle tilde expansion when these paths are actually used,
        // or when the config is fully parsed. For Default, we'll store them as is.
        let default_search_paths = vec![
            PathBuf::from("~/Development"),
            PathBuf::from("~/Development/raganw"),
            PathBuf::from("~/.config"),
            // Add other common paths if known from the bash script
        ];

        Config {
            search_paths: default_search_paths,
            additional_paths: Vec::new(),
            exclude_patterns: Vec::new(), // No default exclude patterns for now, can be added if needed
            debug_mode: false,
            direct_selection: None,
        }
    }
}

impl Config {
    pub fn new() -> Self {
        let cli_args = CliArgs::parse();
        debug!(parsed_cli_args = ?cli_args, "Parsed command line arguments");
        Self::from_args(cli_args)
    }

    // New method to make testing easier
    pub fn from_args(args: CliArgs) -> Self {
        let mut default_config = Config::default();

        // Override default debug_mode if --debug flag is present
        if args.debug {
            default_config.debug_mode = true;
        }

        // Set direct_selection from CLI args
        default_config.direct_selection = args.direct_selection;

        // Here you would override other defaults if CliArgs had more fields
        // For example, if args.additional_paths was Some(paths),
        // you would merge them into default_config.additional_paths.

        default_config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser; // Required for CliArgs::try_parse_from
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
        // Check that other defaults are preserved
        assert_eq!(
            config.search_paths,
            Config::default().search_paths
        );
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
        let cli_args =
            CliArgs::try_parse_from(["tmux-sessionizer", project_name]).unwrap();
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
