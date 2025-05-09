// Add these lines at the beginning of the file
mod config;
mod directory_scanner;
mod fuzzy_finder_interface;
mod git_repository_handler;
mod session_manager;

use crate::config::Config; // Add this line
use tracing::{debug, Level}; // Add debug to tracing imports

// Add this function before main
fn setup_tracing(debug_mode: bool) {
    let level = if debug_mode {
        Level::DEBUG
    } else {
        Level::INFO
    };
    tracing_subscriber::fmt().with_max_level(level).init();
}

// Modify the main function as follows:
fn main() {
    // Parse command-line arguments and create a Config instance
    let config = Config::new();

    // Setup tracing based on the debug_mode from config
    setup_tracing(config.debug_mode);

    tracing::info!("Application started");

    // Log the loaded configuration if debug mode is enabled
    if config.debug_mode {
        tracing::debug!("Loaded configuration: {:?}", config);
    }

    // The original tracing::debug!("Debug mode is enabled."); can be removed
    // as the config log above will show the debug_mode status.
}
