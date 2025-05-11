// Add these lines at the beginning of the file
mod config;
mod container_detector; // Add this line
mod directory_scanner;
mod error; // Add this line
mod fuzzy_finder_interface;
mod git_repository_handler;
mod path_utils; // Add this line
mod session_manager;

use crate::config::Config; // Add this line
use crate::directory_scanner::DirectoryScanner; // Add this line to import DirectoryScanner
use crate::error::Result; // Add this line
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
fn main() -> Result<()> { // Changed to return crate::error::Result for error propagation
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
    let selection_result: Result<Option<SelectedItem>>;

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

            // Find the original DirectoryEntry corresponding to the SelectedItem
            // selected_item.path is the resolved_path of the DirectoryEntry
            let original_dir_entry_opt = scanned_entries.iter().find(|entry| {
                entry.resolved_path == selected_item.path && entry.display_name == selected_item.display_name
            });

            if let Some(original_dir_entry) = original_dir_entry_opt {
                let session_manager = session_manager::SessionManager::new();
                // Create a session_manager::Selection struct, which includes the generated session name
                let sm_selection = session_manager.create_selection_from_directory_entry(original_dir_entry);

                println!("  Session Name: {}", sm_selection.session_name);

                match session_manager.is_tmux_server_running() {
                    Ok(true) => {
                        tracing::info!("Tmux server is running.");
                        match session_manager.session_exists(&sm_selection.session_name) {
                            Ok(true) => {
                                tracing::info!("Session '{}' exists. Switching/Attaching.", sm_selection.session_name);
                                if let Err(e) = session_manager.switch_or_attach_to_session(&sm_selection.session_name) {
                                    eprintln!("Error switching/attaching to session '{}': {}", sm_selection.session_name, e);
                                } else {
                                    println!("Successfully switched/attached to session '{}'.", sm_selection.session_name);
                                }
                            }
                            Ok(false) => {
                                tracing::info!("Session '{}' does not exist. Creating new session.", sm_selection.session_name);
                                match session_manager.create_new_session(&sm_selection.session_name, &sm_selection.path) {
                                    Ok(_) => {
                                        println!("Successfully created session '{}'.", sm_selection.session_name);
                                        tracing::info!("Attempting to switch/attach to newly created session '{}'.", sm_selection.session_name);
                                        // After creating, switch/attach to it.
                                        // create_new_session handles attaching if not in tmux, switch_or_attach handles switching if in tmux.
                                        if let Err(e) = session_manager.switch_or_attach_to_session(&sm_selection.session_name) {
                                            eprintln!("Error switching/attaching to newly created session '{}': {}", sm_selection.session_name, e);
                                        } else {
                                            println!("Successfully switched/attached to new session '{}'.", sm_selection.session_name);
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("Error creating new session '{}': {}", sm_selection.session_name, e);
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Error checking if session '{}' exists: {}", sm_selection.session_name, e);
                            }
                        }
                    }
                    Ok(false) => {
                        println!("Tmux server is not running. Cannot manage session '{}'.", sm_selection.session_name);
                        println!("Please start tmux server to use session management features.");
                    }
                    Err(e) => {
                        eprintln!("Error checking tmux server status: {}", e);
                    }
                }
            } else {
                // This case should ideally not happen if selected_item is derived correctly from scanned_entries
                eprintln!("Error: Could not find the original directory entry for the selection. This is unexpected.");
                eprintln!("Selected item details: Display Name: '{}', Path: '{}'", selected_item.display_name, selected_item.path.display());

            }
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
