use clap::Parser;
use regex::Regex;
use std::path::PathBuf;
use tracing::debug;

#[derive(Parser, Debug)]
#[command(name = "tmux-sessionizer")]
#[command(author, version, about = "A utility for managing tmux sessions based on project directories.", long_about = None)]
struct CliArgs {
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

        let mut default_config = Config::default();

        // Override default debug_mode if --debug flag is present
        if cli_args.debug {
            default_config.debug_mode = true;
        }

        // Set direct_selection from CLI args
        default_config.direct_selection = cli_args.direct_selection;

        // Here you would override other defaults if CliArgs had more fields
        // For example, if cli_args.additional_paths was Some(paths),
        // you would merge them into default_config.additional_paths.

        default_config
    }
}
