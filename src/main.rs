// Add these lines at the beginning of the file
mod config;
mod container_detector; // Add this line
mod directory_scanner;
mod fuzzy_finder_interface;
mod git_repository_handler;
mod path_utils; // Add this line
mod session_manager; 

use crate::config::Config; // Add this line
use crate::directory_scanner::DirectoryScanner; // Add this line to import DirectoryScanner
use crate::fuzzy_finder_interface::{FuzzyFinder, SelectedItem}; // Import FuzzyFinder and SelectedItem
use tracing::Level; // debug is already here, Level is used by setup_tracing

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
fn main() -> anyhow::Result<()> { // Changed to return anyhow::Result for error propagation
    // 1. Parse command-line arguments and create a Config instance
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

    // 2. Create a DirectoryScanner instance and scan directories
    let scanner = DirectoryScanner::new(&config);
    tracing::info!("Starting directory scan via main...");
    let scanned_entries = scanner.scan();
    tracing::info!("Directory scan complete. Found {} entries.", scanned_entries.len());

    // 3. Initialize FuzzyFinder
    let fuzzy_finder = FuzzyFinder::new();
    let selection_result: anyhow::Result<Option<SelectedItem>>;

    // 4. Perform selection (direct or fuzzy)
    if let Some(direct_selection_target) = &config.direct_selection {
        tracing::info!(target = %direct_selection_target, "Attempting direct selection.");
        selection_result = fuzzy_finder.direct_select(&scanned_entries, direct_selection_target);
    } else {
        tracing::info!("No direct selection provided, launching fuzzy finder.");
        if scanned_entries.is_empty() {
            println!("No scannable project directories found. Nothing to select.");
            return Ok(());
        }
        selection_result = fuzzy_finder.select(&scanned_entries); // Pass as a slice reference
    }

    // 5. Handle the selection outcome
    match selection_result {
        Ok(Some(selected_item)) => {
            println!("\nFinal Selection:");
            println!("  Display Name: {}", selected_item.display_name);
            println!("  Path: {}", selected_item.path.display());
            // TODO: Proceed with tmux session management using selected_item
        }
        Ok(None) => {
            println!("\nNo selection made or selection cancelled.");
            if config.direct_selection.is_some() {
                eprintln!("Direct selection target '{}' not found or was ambiguous.", config.direct_selection.as_ref().unwrap());
            }
        }
        Err(e) => {
            eprintln!("\nError during selection process: {}", e);
            // Consider returning the error if main is to propagate it
            // For now, just print and exit gracefully.
            // return Err(e); // If main returns Result
        }
    }
    Ok(())
}
