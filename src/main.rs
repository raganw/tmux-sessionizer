//! # tmux-sessionizer
//!
//! A utility to quickly navigate and manage tmux sessions based on directories,
//! Git repositories, and Git worktrees. It scans configured paths, presents
//! options via a fuzzy finder (or direct selection), and creates or switches
//! to the corresponding tmux session.

mod config;
mod config_init;
mod container_detector;
mod directory_scanner;
mod error;
mod fuzzy_finder_interface;
mod git_repository_handler;
mod logging; // Ensure logging module is declared
mod path_utils;
mod session_manager;

use crate::config::Config;
use crate::directory_scanner::DirectoryScanner;
use crate::error::Result;
use crate::fuzzy_finder_interface::{FuzzyFinder, SelectionResult};

/// Sets up the global tracing subscriber.
///
/// Initializes `tracing_subscriber` based on the provided debug mode flag.
/// Logs are directed to standard output.
///
/// # Arguments
///
/// * `debug_mode` - If `true`, sets the logging level to `DEBUG`, otherwise `INFO`.///
///
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
    // 1. Check if --init flag was provided and handle initialization
    if Config::handle_init_if_requested()? {
        // Initialization was performed, exit early
        return Ok(());
    }

    // 2. Parse command-line arguments, load config file, and create a Config instance
    let config = Config::new()?;

    // Initialize global logging. The guard must stay in scope.
    let log_level_str = if config.debug_mode { "debug" } else { "info" };
    let _logger_guard = logging::init_global_tracing(&config.log_directory, log_level_str)?;

    // This first log message will go to the file via the global subscriber
    tracing::info!("Application started");

    // Log the loaded configuration if debug mode is enabled
    if config.debug_mode {
        tracing::debug!("Loaded configuration: {:?}", config);
    }

    // 3. Create a DirectoryScanner instance and scan directories
    let scanner = DirectoryScanner::new(&config);
    tracing::info!("Starting directory scan via main...");
    let scanned_entries = scanner.scan();
    tracing::info!(
        "Directory scan complete. Found {} entries.",
        scanned_entries.len()
    );

    // 4. Initialize FuzzyFinder
    let selection_result: Result<Option<SelectionResult>>;

    // 5. Perform selection (direct or fuzzy)
    if let Some(direct_selection_target) = &config.direct_selection {
        tracing::info!(target = %direct_selection_target, "Attempting direct selection.");
        // For direct selection, we still use the old interface (only existing projects)
        let direct_result = FuzzyFinder::direct_select(&scanned_entries, direct_selection_target)?;
        selection_result = Ok(direct_result.map(SelectionResult::ExistingProject));
    } else {
        tracing::info!("No direct selection provided, launching fuzzy finder.");
        if scanned_entries.is_empty() {
            tracing::info!("No scannable project directories found. Launching fuzzy finder with new project option only.");
        }
        selection_result = FuzzyFinder::select_with_new_project_option(&scanned_entries, &config.default_new_project_path);
    }

    // 6. Handle the selection outcome
    let selection_option = selection_result?; // Propagate errors from selection process

    if let Some(selection) = selection_option {
        let sm_selection = match selection {
            SelectionResult::ExistingProject(selected_item) => {
                tracing::info!("Final Selection (Existing Project):");
                tracing::info!("  Display Name: {}", selected_item.display_name);
                tracing::info!("  Path: {}", selected_item.path.display());

                let original_dir_entry_opt = scanned_entries.iter().find(|entry| {
                    entry.resolved_path == selected_item.path
                        && entry.display_name == selected_item.display_name
                });

                if let Some(original_dir_entry) = original_dir_entry_opt {
                    session_manager::SessionManager::create_selection_from_directory_entry(
                        original_dir_entry,
                    )
                } else {
                    tracing::error!(
                        "Could not find the original directory entry for the selection. This is unexpected."
                    );
                    tracing::error!(display_name = %selected_item.display_name, path = %selected_item.path.display(), "Selected item details for missing original entry");
                    return Ok(()); // Exit early instead of continuing with invalid state
                }
            }
            SelectionResult::NewProject(new_project_request) => {
                tracing::info!("Creating New Project:");
                tracing::info!("  Project Name: {}", new_project_request.project_name);
                tracing::info!("  Parent Path: {}", new_project_request.parent_path.display());

                match session_manager::SessionManager::create_new_project_directory(
                    &new_project_request.project_name,
                    &new_project_request.parent_path,
                ) {
                    Ok(selection) => selection,
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to create new project directory");
                        return Err(e);
                    }
                }
            }
        };

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
        // Corresponds to Ok(None) from selection_result
        tracing::info!("No selection made or selection cancelled.");
        if config.direct_selection.is_some() {
            tracing::warn!(target = %config.direct_selection.as_ref().unwrap(), "Direct selection target not found or was ambiguous.");
        }
    }
    Ok(())
}
