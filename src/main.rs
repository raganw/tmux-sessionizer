// Add these lines at the beginning of the file
mod config;
mod directory_scanner;
mod fuzzy_finder_interface;
mod git_repository_handler;
mod session_manager;

use crate::config::Config; // Add this line
use crate::directory_scanner::DirectoryScanner; // Add this line to import DirectoryScanner
use tracing::{debug, Level}; // debug is already here, Level is used by setup_tracing

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

    // Create a DirectoryScanner instance
    let scanner = DirectoryScanner::new(&config);

    // Call scan() to get the list of directory entries
    tracing::info!("Starting directory scan via main...");
    let scanned_entries = scanner.scan();
    tracing::info!("Directory scan complete. Found {} entries.", scanned_entries.len());

    // Print the results (for now)
    // This will print regardless of debug_mode for now, as per "print the results (for now)"
    if !scanned_entries.is_empty() {
        println!("\nScanned Directory Entries:");
        for entry in scanned_entries {
            println!(
                "  Display: {}, Path: {}, Resolved: {}",
                entry.display_name,
                entry.path.display(),
                entry.resolved_path.display()
            );
        }
    } else {
        println!("\nNo directory entries found matching the criteria.");
    }
}
