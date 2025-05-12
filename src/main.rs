//! # tmux-sessionizer
//!
//! A utility to quickly navigate and manage tmux sessions based on directories,
//! Git repositories, and Git worktrees. It scans configured paths, presents
//! options via a fuzzy finder (or direct selection), and creates or switches
//! to the corresponding tmux session.

mod config;
mod container_detector;
mod directory_scanner;
mod error;
mod fuzzy_finder_interface;
mod git_repository_handler;
mod path_utils;
mod session_manager;

use crate::config::Config;
use crate::directory_scanner::DirectoryScanner;
use crate::error::Result;
use crate::fuzzy_finder_interface::{FuzzyFinder, SelectedItem};
use tracing::Level;

/// Sets up the global tracing subscriber.
///
/// Initializes `tracing_subscriber` based on the provided debug mode flag.
/// Logs are directed to standard output.
///
/// # Arguments
///
/// * `debug_mode` - If `true`, sets the logging level to `DEBUG`, otherwise `INFO`.
fn setup_tracing(debug_mode: bool) {
    let level = if debug_mode {
        Level::DEBUG
    } else {
        Level::INFO
    };
    tracing_subscriber::fmt().with_max_level(level).init();
}

/// The main entry point of the application.
///
/// Orchestrates the entire process:
/// 1. Parses command-line arguments and initializes configuration.
/// 2. Sets up logging using the `tracing` crate.
/// 3. Scans configured directories to find potential projects (plain directories, Git repos, worktrees).
/// 4. Handles direct selection if provided via arguments, otherwise presents a fuzzy finder interface.
/// 5. Based on the user's selection, determines the target directory and desired tmux session name.
/// 6. Interacts with the tmux server to check for existing sessions, create new ones, or switch/attach.
///
/// # Returns
///
/// * `Result<()>` - Returns `Ok(())` on successful execution, or an `AppError` if any step fails.
fn main() -> Result<()> {
    // 1. Parse command-line arguments and create a Config instance
    let config = Config::new();

    // Setup tracing based on the debug_mode from config
    setup_tracing(config.debug_mode);

    tracing::info!("Application started");

    // Log the loaded configuration if debug mode is enabled
    if config.debug_mode {
        tracing::debug!("Loaded configuration: {:?}", config);
    }

    // 2. Create a DirectoryScanner instance and scan directories
    let scanner = DirectoryScanner::new(&config);
    tracing::info!("Starting directory scan via main...");
    let scanned_entries = scanner.scan();
    tracing::info!(
        "Directory scan complete. Found {} entries.",
        scanned_entries.len()
    );

    // 3. Initialize FuzzyFinder
    let selection_result: Result<Option<SelectedItem>>;

    // 4. Perform selection (direct or fuzzy)
    if let Some(direct_selection_target) = &config.direct_selection {
        tracing::info!(target = %direct_selection_target, "Attempting direct selection.");
        selection_result = FuzzyFinder::direct_select(&scanned_entries, direct_selection_target);
    } else {
        tracing::info!("No direct selection provided, launching fuzzy finder.");
        if scanned_entries.is_empty() {
            tracing::info!("No scannable project directories found. Nothing to select.");
            return Ok(());
        }
        selection_result = FuzzyFinder::select(&scanned_entries); // Pass as a slice reference
    }

    // 5. Handle the selection outcome
    let selected_item_option = selection_result?; // Propagate errors from selection process

    if let Some(selected_item) = selected_item_option {
        tracing::info!("Final Selection:");
        tracing::info!("  Display Name: {}", selected_item.display_name);
        tracing::info!("  Path: {}", selected_item.path.display());

        let original_dir_entry_opt = scanned_entries.iter().find(|entry| {
            entry.resolved_path == selected_item.path
                && entry.display_name == selected_item.display_name
        });

        if let Some(original_dir_entry) = original_dir_entry_opt {
            let sm_selection =
                session_manager::SessionManager::create_selection_from_directory_entry(
                    original_dir_entry,
                );

            tracing::info!("  Session Name: {}", sm_selection.session_name);

            match session_manager::SessionManager::is_tmux_server_running() {
                Ok(true) => {
                    tracing::info!("Tmux server is running.");
                    match session_manager::SessionManager::session_exists(
                        &sm_selection.session_name,
                    ) {
                        Ok(true) => {
                            tracing::info!(session_name = %sm_selection.session_name, "Session exists. Switching/Attaching.");
                            session_manager::SessionManager::switch_or_attach_to_session(
                                &sm_selection.session_name,
                            )?;
                            tracing::info!(session_name = %sm_selection.session_name, "Successfully switched/attached to session.");
                        }
                        Ok(false) => {
                            tracing::info!(session_name = %sm_selection.session_name, "Session does not exist. Creating new session.");
                            session_manager::SessionManager::create_new_session(
                                &sm_selection.session_name,
                                &sm_selection.path,
                            )?;
                            tracing::info!(session_name = %sm_selection.session_name, "Successfully created session.");

                            tracing::info!(session_name = %sm_selection.session_name, "Attempting to switch/attach to newly created session.");
                            session_manager::SessionManager::switch_or_attach_to_session(
                                &sm_selection.session_name,
                            )?;
                            tracing::info!(session_name = %sm_selection.session_name, "Successfully switched/attached to new session.");
                        }
                        Err(e) => {
                            // Error checking session existence is distinct from action errors
                            tracing::error!(session_name = %sm_selection.session_name, error = %e, "Error checking if session exists.");
                        }
                    }
                }
                Ok(false) => {
                    tracing::warn!(session_name = %sm_selection.session_name, "Tmux server is not running. Cannot manage session.");
                    tracing::info!("Please start tmux server to use session management features.");
                }
                Err(e) => {
                    // Error checking server status
                    tracing::error!(error = %e, "Error checking tmux server status.");
                }
            }
        } else {
            tracing::error!(
                "Could not find the original directory entry for the selection. This is unexpected."
            );
            tracing::error!(display_name = %selected_item.display_name, path = %selected_item.path.display(), "Selected item details for missing original entry");
        }
    } else {
        // Corresponds to Ok(None) from selection_result
        tracing::info!("No selection made or selection cancelled.");
        if config.direct_selection.is_some() {
            tracing::warn!(target = %config.direct_selection.as_ref().unwrap(), "Direct selection target not found or was ambiguous.");
        }
    }
    Ok(())
}
