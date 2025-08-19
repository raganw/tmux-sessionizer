//! Handles the user interface for selecting a directory, either through a fuzzy finder
//! (using the `skim` library) or by direct matching based on user input.
//!
//! This module provides the `FuzzyFinder` struct and associated methods to:
//! - Format directory entries for display.
//! - Prepare input for the `skim` fuzzy finder.
//! - Run the `skim` interface and process user selection.
//! - Implement direct selection logic based on various matching strategies.
//! - Define the `SelectedItem` struct to represent the user's choice.

use crate::directory_scanner::DirectoryEntry;
use crate::error::{AppError, Result};
use skim::prelude::*;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use tracing::{debug, warn};

/// Represents an item selected by the user, either via the fuzzy finder or direct selection.
///
/// This struct holds the necessary information to proceed with creating or switching
/// to a tmux session corresponding to the selected directory.
#[derive(Debug, Clone)]
pub struct SelectedItem {
    /// The name displayed to the user in the selection list (e.g., "`my_project`" or "[repo] worktree").
    pub display_name: String,
    /// The canonicalized, absolute filesystem path corresponding to the selected item.
    /// This path is used as the target directory for the tmux session.
    pub path: PathBuf,
}

/// Represents a request to create a new project.
///
/// This is used when the user selects a new project to be created rather than
/// selecting an existing directory.
#[derive(Debug, Clone)]
pub struct NewProjectRequest {
    /// The name of the new project to create.
    pub project_name: String,
    /// The path where the new project directory should be created.
    pub parent_path: PathBuf,
}

/// Represents the result of a user's selection from the fuzzy finder.
#[derive(Debug, Clone)]
pub enum SelectionResult {
    /// User selected an existing directory/project.
    ExistingProject(SelectedItem),
    /// User requested to create a new project.
    NewProject(NewProjectRequest),
}

/// Provides methods for interacting with the user to select a directory.
///
/// This includes presenting a list of directories via a fuzzy finder (`skim`)
/// or attempting to directly match a user-provided string against the available directories.
pub struct FuzzyFinder {}

impl FuzzyFinder {
    /// Formats a `DirectoryEntry` for display in the `skim` fuzzy finder.
    ///
    /// The output format is `display_name\tresolved_path`. The `resolved_path` is included
    /// primarily for potential use in `skim`'s preview window or if `skim` needs to parse
    /// the path itself, although the primary selection mechanism relies on parsing this
    /// line format after `skim` returns the selected line.
    ///
    /// # Arguments
    ///
    /// * `entry` - The `DirectoryEntry` to format.
    ///
    /// # Returns
    ///
    /// A `String` formatted for `skim` input.
    fn format_directory_entry_for_skim(entry: &DirectoryEntry) -> String {
        format!("{}\t{}", entry.display_name, entry.resolved_path.display())
    }

    /// Prepares the input string for the `skim` fuzzy finder by formatting each `DirectoryEntry`.
    ///
    /// Takes a slice of `DirectoryEntry` items, formats each one using
    /// [`format_directory_entry_for_skim`](#method.format_directory_entry_for_skim),
    /// and joins them into a single newline-separated string suitable for `skim`.
    ///
    /// # Arguments
    ///
    /// * `entries` - A slice of `DirectoryEntry` items to be presented in the fuzzy finder.
    ///
    /// # Returns
    ///
    /// A `String` containing all formatted entries, separated by newlines.
    pub fn prepare_skim_input(entries: &[DirectoryEntry]) -> String {
        entries
            .iter()
            .map(FuzzyFinder::format_directory_entry_for_skim)
            .collect::<Vec<String>>()
            .join("\n")
    }

    /// Runs the `skim` fuzzy finder to allow the user to select a directory entry or create a new project.
    ///
    /// Takes a slice of `DirectoryEntry` items, prepares the input for `skim`,
    /// runs the `skim` interface, and processes the user's selection.
    /// Additionally supports creating new projects when the user types a name starting with "+".
    ///
    /// # Arguments
    ///
    /// * `entries` - A slice of `DirectoryEntry` items to present to the user.
    /// * `default_new_project_path` - The default path where new projects should be created.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(SelectionResult))` containing the details of the user's selection or new project request.
    /// * `Ok(None)` if the user cancelled the selection (e.g., pressed ESC) or if no entries were provided.
    /// * `Err(AppError::Finder)` if there was an error running `skim` or parsing its output.
    ///
    /// # Errors
    ///
    /// Returns `AppError::Finder` if:
    /// - `skim` options fail to build.
    /// - `skim` execution itself fails.
    /// - The selected line from `skim` cannot be parsed into the expected format.
    pub fn select_with_new_project_option(
        entries: &[DirectoryEntry],
        default_new_project_path: &std::path::Path,
    ) -> Result<Option<SelectionResult>> {
        if entries.is_empty() {
            debug!("No entries provided to fuzzy finder, returning None.");
            return Ok(None);
        }

        // Add a special entry for creating new projects
        let mut skim_input = "+ Create New Project...\t<NEW_PROJECT>\n".to_string();
        skim_input.push_str(&Self::prepare_skim_input(entries));

        debug!(
            "Skim input prepared with {} entries plus new project option.",
            entries.len()
        );

        // Configure Skim options
        let options = SkimOptionsBuilder::default()
            .height("100%".to_string())
            .multi(false) // Single selection mode
            .prompt("Select project (or + to create new): ".to_string())
            .build()
            .map_err(|e| AppError::Finder(format!("Failed to build Skim options: {e}")))?;

        // Create an item reader from the prepared input string
        let item_reader = SkimItemReader::default();
        let items = item_reader.of_bufread(Cursor::new(skim_input));

        // Run Skim and process the output
        let skim_output = Skim::run_with(&options, Some(items)).ok_or_else(|| {
            AppError::Finder("Skim execution failed or was cancelled by user initially".to_string())
        })?;

        if skim_output.is_abort {
            debug!("Skim selection aborted by user (e.g., ESC pressed).");
            return Ok(None);
        }

        let selected_items = skim_output.selected_items;

        if selected_items.is_empty() {
            debug!("No items selected in Skim.");
            return Ok(None);
        }

        // We expect only one selected item due to `multi(false)`
        let selected_skim_item = selected_items.first().ok_or_else(|| {
            AppError::Finder(
                "Skim reported selection but no items found in selected_items list".to_string(),
            )
        })?;

        let selected_line = selected_skim_item.output().to_string();
        debug!("Skim selected line: '{}'", selected_line);

        // Check if user wants to create a new project
        if selected_line.starts_with("+ Create New Project") {
            // Prompt user for project name
            return Self::handle_new_project_creation(default_new_project_path);
        }

        // Parse the selected line (format: "display_name\tresolved_path")
        let parts: Vec<&str> = selected_line.splitn(2, '\t').collect();
        if parts.len() == 2 {
            let display_name = parts[0].to_string();
            let path_str = parts[1];
            let path = PathBuf::from(path_str);

            debug!(
                "Parsed selection - Display: '{}', Path: '{}'",
                display_name,
                path.display()
            );
            Ok(Some(SelectionResult::ExistingProject(SelectedItem {
                display_name,
                path,
            })))
        } else {
            Err(AppError::Finder(format!(
                "Selected line from Skim has unexpected format (expected 'display\\tpath'): '{selected_line}'"
            )))
        }
    }

    fn handle_new_project_creation(
        default_new_project_path: &std::path::Path,
    ) -> Result<Option<SelectionResult>> {
        use std::io::{self, Write};

        print!("Enter new project name: ");
        io::stdout()
            .flush()
            .map_err(|e| AppError::Finder(format!("Failed to flush stdout: {e}")))?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|e| AppError::Finder(format!("Failed to read from stdin: {e}")))?;

        let project_name = input.trim();
        if project_name.is_empty() {
            debug!("Empty project name provided, cancelling creation");
            return Ok(None);
        }

        // Validate project name (basic validation)
        if project_name.contains('/') || project_name.contains('\\') {
            return Err(AppError::Finder(
                "Project name cannot contain path separators".to_string(),
            ));
        }

        debug!("User requested to create new project: '{}'", project_name);
        Ok(Some(SelectionResult::NewProject(NewProjectRequest {
            project_name: project_name.to_string(),
            parent_path: default_new_project_path.to_path_buf(),
        })))
    }

    /// Attempts to find a unique `DirectoryEntry` based on a user-provided search string,
    /// bypassing the interactive fuzzy finder.
    ///
    /// This function implements a prioritized matching strategy:
    ///
    /// 1.  **Canonical Path Match:** Checks if `search_target_raw`, when treated as a path
    ///     and canonicalized, exactly matches the `resolved_path` of any entry.
    /// 2.  **Original Path Match:** Checks if `search_target_raw`, treated as a path,
    ///     exactly matches the original `path` (before resolution/canonicalization) of any entry.
    ///     This is useful for matching symlink paths directly.
    /// 3.  **Suffix Match:** Checks if the `resolved_path` of any entry ends with `search_target_raw`
    ///     (interpreted as a path suffix). Returns an error if multiple entries match.
    /// 4.  **Display Name Match:** Checks if the `display_name` of any entry exactly matches
    ///     `search_target_raw`. Returns an error if multiple entries match.
    /// 5.  **Filename Match:** Checks if the filename component of the `resolved_path` of any
    ///     entry exactly matches `search_target_raw`. Returns an error if multiple entries match.
    ///
    /// # Arguments
    ///
    /// * `entries` - A slice of `DirectoryEntry` items representing the available choices.
    /// * `search_target_raw` - The string provided by the user to identify the desired directory.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(SelectedItem))` if a unique match is found according to the strategies above.
    /// * `Ok(None)` if no match is found across all strategies.
    /// * `Err(AppError::Finder)` if multiple entries match ambiguously for suffix, display name,
    ///   or filename strategies.
    /// * `Err(AppError::Io)` if an I/O error occurs during path canonicalization (and the path
    ///   looks like more than just a simple name).
    ///
    /// # Errors
    ///
    /// Returns `AppError::Finder` for ambiguous matches.
    /// May return `AppError::Io` indirectly via `fs::canonicalize`.
    #[allow(clippy::too_many_lines)]
    pub fn direct_select(
        entries: &[DirectoryEntry],
        search_target_raw: &str,
    ) -> Result<Option<SelectedItem>> {
        if entries.is_empty() {
            debug!("Direct selection: No entries provided, cannot select.");
            return Ok(None);
        }

        debug!(
            "Direct selection: Attempting to find '{}' in {} entries.",
            search_target_raw,
            entries.len()
        );

        let search_target_path = PathBuf::from(search_target_raw);

        // Priority 1: Canonical Path Match
        match fs::canonicalize(&search_target_path) {
            Ok(canonical_target) => {
                if let Some(entry) = entries.iter().find(|e| e.resolved_path == canonical_target) {
                    debug!(
                        "Direct selection: Matched canonical path '{}' to entry '{}' ({})",
                        canonical_target.display(),
                        entry.display_name,
                        entry.resolved_path.display()
                    );
                    return Ok(Some(SelectedItem {
                        display_name: entry.display_name.clone(),
                        path: entry.resolved_path.clone(),
                    }));
                }
            }
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound
                    || search_target_path.components().count() > 1
                    || search_target_path.is_absolute()
                {
                    debug!(
                        "Direct selection: Failed to canonicalize search target '{}': {}. Continuing with other matching strategies.",
                        search_target_path.display(),
                        e
                    );
                } else {
                    debug!(
                        "Direct selection: Search target '{}' not found as a direct canonicalizable path (error: {}). Trying other strategies.",
                        search_target_path.display(),
                        e
                    );
                }
            }
        }

        // Priority 2: Exact match on `entry.path` (original path before resolution)
        if let Some(entry) = entries.iter().find(|e| e.path == search_target_path) {
            debug!(
                "Direct selection: Matched original path '{}' to entry '{}' ({})",
                search_target_path.display(),
                entry.display_name,
                entry.path.display()
            );
            return Ok(Some(SelectedItem {
                display_name: entry.display_name.clone(),
                path: entry.resolved_path.clone(), // Still use resolved_path for tmux
            }));
        }

        // Priority 3: Suffix match on `entry.resolved_path`
        let suffix_matches: Vec<&DirectoryEntry> = entries
            .iter()
            .filter(|e| e.resolved_path.ends_with(&search_target_path))
            .collect();

        match suffix_matches.len() {
            1 => {
                let entry = suffix_matches[0];
                debug!(
                    "Direct selection: Matched suffix '{}' to entry '{}' ({})",
                    search_target_path.display(),
                    entry.display_name,
                    entry.resolved_path.display()
                );
                return Ok(Some(SelectedItem {
                    display_name: entry.display_name.clone(),
                    path: entry.resolved_path.clone(),
                }));
            }
            count if count > 1 => {
                let matched_paths: Vec<String> = suffix_matches
                    .iter()
                    .map(|e| e.resolved_path.display().to_string())
                    .collect();
                warn!(
                    "Direct selection: Search target '{}' is ambiguous by suffix, matched: {:?}",
                    search_target_raw, matched_paths
                );
                return Err(AppError::Finder(format!(
                    "Search target '{}' is ambiguous: {} entries end with this path. Matches: {:?}",
                    search_target_raw,
                    suffix_matches.len(),
                    matched_paths
                )));
            }
            _ => {} // No match or 0 matches, continue to next strategy
        }

        // Priority 4: Exact match on `entry.display_name`
        let display_name_matches: Vec<&DirectoryEntry> = entries
            .iter()
            .filter(|e| e.display_name == search_target_raw)
            .collect();
        match display_name_matches.len() {
            1 => {
                let entry = display_name_matches[0];
                debug!(
                    "Direct selection: Matched display name '{}' to entry '{}' ({})",
                    search_target_raw,
                    entry.display_name,
                    entry.resolved_path.display()
                );
                return Ok(Some(SelectedItem {
                    display_name: entry.display_name.clone(),
                    path: entry.resolved_path.clone(),
                }));
            }
            count if count > 1 => {
                let matched_displays: Vec<String> = display_name_matches
                    .iter()
                    .map(|e| format!("{} ({})", e.display_name, e.resolved_path.display()))
                    .collect();
                warn!(
                    "Direct selection: Search target '{}' is ambiguous by display name, matched: {:?}",
                    search_target_raw, matched_displays
                );
                return Err(AppError::Finder(format!(
                    "Search target '{}' is ambiguous: {} entries have this display name. Matches: {:?}",
                    search_target_raw,
                    display_name_matches.len(),
                    matched_displays
                )));
            }
            _ => {} // No match or 0 matches, continue
        }

        // Priority 5: Filename match on `entry.resolved_path.file_name()`
        let filename_matches: Vec<&DirectoryEntry> = entries
            .iter()
            .filter(|e| {
                e.resolved_path
                    .file_name()
                    .is_some_and(|name| name == search_target_raw)
            })
            .collect();
        match filename_matches.len() {
            1 => {
                let entry = filename_matches[0];
                debug!(
                    "Direct selection: Matched filename '{}' to entry '{}' ({})",
                    search_target_raw,
                    entry.display_name,
                    entry.resolved_path.display()
                );
                return Ok(Some(SelectedItem {
                    display_name: entry.display_name.clone(),
                    path: entry.resolved_path.clone(),
                }));
            }
            count if count > 1 => {
                let matched_filenames: Vec<String> = filename_matches
                    .iter()
                    .map(|e| {
                        format!(
                            "{} ({})",
                            e.resolved_path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy(),
                            e.resolved_path.display()
                        )
                    })
                    .collect();
                warn!(
                    "Direct selection: Search target '{}' is ambiguous by filename, matched: {:?}",
                    search_target_raw, matched_filenames
                );
                return Err(AppError::Finder(format!(
                    "Search target '{}' is ambiguous: {} entries have this filename. Matches: {:?}",
                    search_target_raw,
                    filename_matches.len(),
                    matched_filenames
                )));
            }
            _ => {} // No match or 0 matches, continue
        }

        debug!(
            "Direct selection: No unique match found for '{}'",
            search_target_raw
        );
        Ok(None)
    }
}

#[cfg(test)]
mod tests;
