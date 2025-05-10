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
            // Determine type string for display
            let type_str = match entry.entry_type {
                directory_scanner::DirectoryType::Plain => "Plain",
                directory_scanner::DirectoryType::GitRepository => "Git Repo",
                directory_scanner::DirectoryType::GitWorktree { .. } => "Git Worktree",
                directory_scanner::DirectoryType::GitWorktreeContainer => "Git Worktree Container",
            };

            print!(
                "  Type: {:<25} | Display: {:<40} | Path: {}",
                type_str,
                entry.display_name,
                entry.path.display()
            );

            // Show parent repository for worktrees
            if let directory_scanner::DirectoryType::GitWorktree { ref main_worktree_path } = entry.entry_type {
                if let Some(parent_path) = &entry.parent_path {
                    print!(" | Main Repo: {}", parent_path.display());
                    // Ensure consistency: main_worktree_path from enum should match parent_path from struct
                    if parent_path != main_worktree_path && cfg!(debug_assertions) {
                        // This assertion helps catch discrepancies during development/debugging.
                        // It's good practice to ensure these two paths are indeed the same.
                        // In release builds, this assertion will be compiled out.
                        // For production, a warning log might be more appropriate if they can diverge.
                        tracing::warn!(
                            "Mismatch between entry.parent_path ({}) and enum's main_worktree_path ({}) for worktree: {}",
                            parent_path.display(), main_worktree_path.display(), entry.path.display()
                        );
                        // Depending on strictness, one might prefer to panic or handle this as an error.
                        // For now, we'll just log a warning in debug builds if they differ.
                        // The spec implies parent_path is the one to use for display if available.
                    }
                } else {
                    // Fallback if parent_path is None, though it should be set for worktrees.
                    print!(" | Main Repo Path (from type): {}", main_worktree_path.display());
                }
            }
            // Optionally, show resolved path if different or for debugging
            // if entry.path != entry.resolved_path {
            //     print!(" | Resolved: {}", entry.resolved_path.display());
            // }
            println!(); // Newline for next entry
        }
    } else {
        println!("\nNo directory entries found matching the criteria.");
    }
}
