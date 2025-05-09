// Add these lines at the beginning of the file
mod config;
mod directory_scanner;
mod fuzzy_finder_interface;
mod git_repository_handler;
mod session_manager;

use tracing::Level;

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
    // For now, let's enable debug mode by default for tracing setup.
    // This will be replaced by config-driven value later.
    setup_tracing(true);

    tracing::info!("Application started");
    // The original println!("Hello, world!"); should be removed or commented out.
    // println!("Hello, world!");
    tracing::debug!("Debug mode is enabled."); // Example debug message
}
