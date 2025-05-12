use crate::directory_scanner::DirectoryEntry;
use crate::error::{AppError, Result}; // Add AppError and Result
use skim::prelude::*; // Add skim prelude
use std::fs; // For fs::canonicalize
use std::io::Cursor; // To create a BufRead from String for Skim
use std::path::PathBuf;
use tracing::{debug, warn}; // For logging, added warn

/// Represents an item selected by the user from the fuzzy finder.
#[derive(Debug, Clone)]
pub struct SelectedItem {
    /// The display name as it was shown in the fuzzy finder.
    pub display_name: String,
    /// The resolved filesystem path for the selected item. This path will be used
    /// for creating or switching to a tmux session.
    pub path: PathBuf,
}

/// Handles the fuzzy finding process, including formatting entries for display
/// and preparing input for the Skim library.
pub struct FuzzyFinder {}

impl FuzzyFinder {
    /// Creates a new instance of `FuzzyFinder`.
    pub fn new() -> Self {
        Self {}
    }

    /// Formats a single directory entry for display in the Skim fuzzy finder.
    ///
    /// The format is `display_name\tresolved_path`, which is consistent with
    /// how the original bash script formatted entries for `fzf`.
    ///
    /// # Arguments
    ///
    /// * `entry` - A reference to the `DirectoryEntry` to format.
    ///
    /// # Returns
    ///
    /// A string representation of the directory entry suitable for Skim.
    fn format_directory_entry_for_skim(entry: &DirectoryEntry) -> String {
        format!("{}\t{}", entry.display_name, entry.resolved_path.display())
    }

    /// Prepares the complete input string for Skim from a list of directory entries.
    ///
    /// Each `DirectoryEntry` is formatted using `format_directory_entry_for_skim`
    /// and then all formatted strings are joined by newline characters. This resulting
    /// string can be passed to Skim's item reader.
    ///
    /// # Arguments
    ///
    /// * `entries` - A slice of `DirectoryEntry` items to prepare.
    ///
    /// # Returns
    ///
    /// A single string where each line is a formatted directory entry.
    pub fn prepare_skim_input(&self, entries: &[DirectoryEntry]) -> String {
        entries
            .iter()
            .map(FuzzyFinder::format_directory_entry_for_skim)
            .collect::<Vec<String>>()
            .join("\n")
    }

    /// Presents the directory entries to the user via Skim for fuzzy selection.
    ///
    /// # Arguments
    ///
    /// * `entries` - A vector of `DirectoryEntry` items to present.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(SelectedItem))` if the user makes a selection.
    /// * `Ok(None)` if the user cancels the selection (e.g., by pressing Esc).
    /// * `Err(AppError)` if an error occurs during the Skim process or parsing the selection.
    pub fn select(&self, entries: &[DirectoryEntry]) -> Result<Option<SelectedItem>> {
        if entries.is_empty() {
            debug!("No entries provided to fuzzy finder, returning None.");
            return Ok(None);
        }

        let skim_input = self.prepare_skim_input(entries); // Pass slice directly
        debug!("Skim input prepared with {} entries.", entries.len());
        if skim_input.is_empty() && !entries.is_empty() {
            // This case might happen if all entries somehow format to empty strings,
            // though format_directory_entry_for_skim should prevent this.
            // Or if entries are not empty but prepare_skim_input results in an empty string.
            debug!("Skim input is empty despite non-empty entries. Returning None.");
            return Ok(None);
        }
        if skim_input.is_empty() && entries.is_empty() {
            // Already handled by the first check, but being explicit.
            return Ok(None);
        }

        // Configure Skim options
        let options = SkimOptionsBuilder::default()
            .height("50%".to_string())
            .multi(false) // Single selection mode
            .prompt("Select project: ".to_string())
            // .header(Some("Choose a directory:")) // Alternative to prompt
            // .preview(Some("")) // Enable preview window, command can be set
            // .delimiter(Some("\t")) // If Skim needs to parse fields internally
            .build()
            .map_err(|e| AppError::Finder(format!("Failed to build Skim options: {e}")))?;

        // Create an item reader from the prepared input string
        let item_reader = SkimItemReader::default();
        let items = item_reader.of_bufread(Cursor::new(skim_input));

        // Run Skim and process the output
        // Skim::run_with returns Option<SkimOutput>
        let skim_output = Skim::run_with(&options, Some(items)).ok_or_else(|| {
            AppError::Finder("Skim execution failed or was cancelled by user initially".to_string())
        })?;
        // If Skim::run_with returns None, it means skim was aborted (e.g. ESC) before selection loop started.
        // If it returns Some(output), then output.is_abort indicates if ESC was pressed during selection.

        if skim_output.is_abort {
            debug!("Skim selection aborted by user (e.g., ESC pressed).");
            return Ok(None);
        }

        let selected_items = skim_output.selected_items;

        if selected_items.is_empty() {
            // This can happen if the user exits Skim without making a selection
            // (e.g., Ctrl-C, or if is_abort was false but nothing was selected).
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

        // Parse the selected line (format: "display_name\tresolved_path")
        // The `output()` method on SkimItem already gives the full line that was fed to Skim.
        let parts: Vec<&str> = selected_line.splitn(2, '\t').collect(); // Split only on the first tab
        if parts.len() == 2 {
            let display_name = parts[0].to_string();
            let path_str = parts[1];
            let path = PathBuf::from(path_str);

            debug!(
                "Parsed selection - Display: '{}', Path: '{}'",
                display_name,
                path.display()
            );
            Ok(Some(SelectedItem { display_name, path }))
        } else {
            Err(AppError::Finder(format!(
                "Selected line from Skim has unexpected format (expected 'display\\tpath'): '{selected_line}'"
            )))
        }
    }

    /// Attempts to directly select a directory entry based on a search target string.
    ///
    /// This method tries several strategies to find a unique match:
    /// 1. Exact match on the canonicalized version of `search_target_raw` against `entry.resolved_path`.
    /// 2. Exact match on `search_target_raw` (as a path) against `entry.path` (original path).
    /// 3. Suffix match: `entry.resolved_path` ends with `search_target_raw`.
    /// 4. Exact match: `entry.display_name` equals `search_target_raw`.
    /// 5. Filename match: `entry.resolved_path.file_name()` equals `search_target_raw`.
    ///
    /// If multiple entries match by suffix, display name, or filename, an error is returned
    /// indicating ambiguity.
    ///
    /// # Arguments
    ///
    /// * `entries` - A slice of `DirectoryEntry` items to search within.
    /// * `search_target_raw` - The string to search for, typically from a command-line argument.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(SelectedItem))` if a unique match is
    /// found.
    /// * `Ok(None)` if no match is found.
    /// * `Err(AppError)` if an error occurs (e.g., ambiguity, I/O error during canonicalization).
    pub fn direct_select(
        &self,
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

        if suffix_matches.len() == 1 {
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
        } else if suffix_matches.len() > 1 {
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

        // Priority 4: Exact match on `entry.display_name`
        let display_name_matches: Vec<&DirectoryEntry> = entries
            .iter()
            .filter(|e| e.display_name == search_target_raw)
            .collect();
        if display_name_matches.len() == 1 {
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
        } else if display_name_matches.len() > 1 {
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

        // Priority 5: Filename match on `entry.resolved_path.file_name()`
        let filename_matches: Vec<&DirectoryEntry> = entries
            .iter()
            .filter(|e| {
                e.resolved_path
                    .file_name()
                    .is_some_and(|name| name == search_target_raw)
            })
            .collect();
        if filename_matches.len() == 1 {
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
        } else if filename_matches.len() > 1 {
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

        debug!(
            "Direct selection: No unique match found for '{}'",
            search_target_raw
        );
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directory_scanner::DirectoryType; // For creating test DirectoryEntry
    use tempfile::tempdir; // For creating test directories and symlinks

    #[test]
    fn test_format_directory_entry_for_skim_plain() {
        let entry = DirectoryEntry {
            path: PathBuf::from("/original/path/to/project_a"),
            resolved_path: PathBuf::from("/resolved/path/to/project_a"),
            display_name: "project_a".to_string(),
            entry_type: DirectoryType::Plain,
            parent_path: None,
        };
        assert_eq!(
            FuzzyFinder::format_directory_entry_for_skim(&entry),
            "project_a\t/resolved/path/to/project_a"
        );
    }

    #[test]
    fn test_format_directory_entry_for_skim_git_repo() {
        let entry = DirectoryEntry {
            path: PathBuf::from("/original/git_repo"),
            resolved_path: PathBuf::from("/resolved/git_repo"),
            display_name: "git_repo".to_string(),
            entry_type: DirectoryType::GitRepository,
            parent_path: None,
        };
        assert_eq!(
            FuzzyFinder::format_directory_entry_for_skim(&entry),
            "git_repo\t/resolved/git_repo"
        );
    }

    #[test]
    fn test_format_directory_entry_for_skim_worktree() {
        let main_repo_path = PathBuf::from("/resolved/main_repo");
        let entry = DirectoryEntry {
            path: PathBuf::from("/original/main_repo/worktree_x"),
            resolved_path: PathBuf::from("/resolved/main_repo/worktree_x"),
            display_name: "[main_repo] worktree_x".to_string(),
            entry_type: DirectoryType::GitWorktree {
                main_worktree_path: main_repo_path.clone(),
            },
            parent_path: Some(main_repo_path),
        };
        assert_eq!(
            FuzzyFinder::format_directory_entry_for_skim(&entry),
            "[main_repo] worktree_x\t/resolved/main_repo/worktree_x"
        );
    }

    #[test]
    fn test_prepare_skim_input_empty() {
        let fuzzy_finder = FuzzyFinder::new();
        let entries = Vec::new();
        assert_eq!(fuzzy_finder.prepare_skim_input(&entries), "");
    }

    #[test]
    fn test_prepare_skim_input_multiple_entries() {
        let fuzzy_finder = FuzzyFinder::new();
        let entries = vec![
            DirectoryEntry {
                path: PathBuf::from("/orig/p1"),
                resolved_path: PathBuf::from("/res/p1"),
                display_name: "p1".to_string(),
                entry_type: DirectoryType::Plain,
                parent_path: None,
            },
            DirectoryEntry {
                path: PathBuf::from("/orig/p2"),
                resolved_path: PathBuf::from("/res/p2"),
                display_name: "p2_display".to_string(),
                entry_type: DirectoryType::GitRepository,
                parent_path: None,
            },
        ];
        let expected_output = "p1\t/res/p1\np2_display\t/res/p2";
        assert_eq!(fuzzy_finder.prepare_skim_input(&entries), expected_output);
    }

    #[test]
    fn test_select_with_empty_entries_returns_ok_none() {
        let finder = FuzzyFinder::new();
        let entries = Vec::new();
        let result = finder.select(&entries);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    // Helper to create DirectoryEntry for direct_select tests
    fn new_test_entry(p_str: &str, rp_str: &str, dn_str: &str) -> DirectoryEntry {
        DirectoryEntry {
            path: PathBuf::from(p_str),
            resolved_path: PathBuf::from(rp_str),
            display_name: dn_str.to_string(),
            entry_type: DirectoryType::Plain,
            parent_path: None,
        }
    }

    #[test]
    fn test_direct_select_empty_entries() {
        let finder = FuzzyFinder::new();
        let entries = Vec::new();
        let result = finder.direct_select(&entries, "anything");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_direct_select_no_match() {
        let finder = FuzzyFinder::new();
        let entries = vec![new_test_entry(
            "/path/to/project_a",
            "/resolved/project_a",
            "project_a",
        )];
        let result = finder.direct_select(&entries, "nonexistent_project");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_direct_select_canonical_path_match() {
        let finder = FuzzyFinder::new();
        let temp_dir = tempdir().unwrap();
        let project_path = temp_dir.path().join("my_project");
        fs::create_dir(&project_path).unwrap();
        let canonical_project_path = fs::canonicalize(&project_path).unwrap();

        let entries = vec![DirectoryEntry {
            path: project_path.clone(),
            resolved_path: canonical_project_path.clone(),
            display_name: "my_project_display".to_string(),
            entry_type: DirectoryType::Plain,
            parent_path: None,
        }];
        let result = finder.direct_select(&entries, project_path.to_str().unwrap());
        assert!(result.is_ok());
        let selection = result.unwrap().expect("Should have found a selection");
        assert_eq!(selection.display_name, "my_project_display");
        assert_eq!(selection.path, canonical_project_path);
        fs::remove_dir(&project_path).unwrap();
    }

    #[test]
    fn test_direct_select_original_path_match() {
        let finder = FuzzyFinder::new();
        let temp_target_dir = tempdir().unwrap();
        let project_target_path = temp_target_dir.path().join("actual_project");
        fs::create_dir(&project_target_path).unwrap();
        let canonical_project_target_path = fs::canonicalize(&project_target_path).unwrap();

        let temp_link_dir = tempdir().unwrap();
        let symlink_path = temp_link_dir.path().join("project_link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&project_target_path, &symlink_path).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&project_target_path, &symlink_path).unwrap();

        // Skip test if symlink creation failed (e.g. permissions on Windows, or not supported)
        if !symlink_path.exists() && !symlink_path.is_symlink() {
            tracing::warn!(
                "Symlink creation failed or not supported, skipping test_direct_select_original_path_match"
            );
            return;
        }

        let entries = vec![DirectoryEntry {
            path: symlink_path.clone(),
            resolved_path: canonical_project_target_path.clone(),
            display_name: "linked_project".to_string(),
            entry_type: DirectoryType::Plain,
            parent_path: None,
        }];
        let result = finder.direct_select(&entries, symlink_path.to_str().unwrap());
        assert!(result.is_ok(), "Result was: {:?}", result.err());
        let selection = result
            .unwrap()
            .expect("Should have found a selection by original path");
        assert_eq!(selection.display_name, "linked_project");
        assert_eq!(selection.path, canonical_project_target_path);

        // fs::remove_file on symlink, or remove_dir if it's a directory symlink
        #[cfg(unix)]
        fs::remove_file(&symlink_path)
            .unwrap_or_else(|e| tracing::warn!("Failed to remove symlink file: {}", e));
        #[cfg(windows)]
        fs::remove_dir(&symlink_path)
            .unwrap_or_else(|e| tracing::warn!("Failed to remove symlink dir: {}", e));
        fs::remove_dir(&project_target_path).unwrap();
    }

    #[test]
    fn test_direct_select_suffix_match_unique() {
        let finder = FuzzyFinder::new();
        let entries = vec![
            new_test_entry(
                "/p/to/project_a",
                "/resolved/path/to/project_a",
                "project_a",
            ),
            new_test_entry(
                "/a/p/project_b",
                "/resolved/another/path/project_b",
                "project_b",
            ),
        ];
        let result = finder.direct_select(&entries, "to/project_a");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().unwrap().display_name, "project_a");
    }

    #[test]
    fn test_direct_select_display_name_match_unique() {
        let finder = FuzzyFinder::new();
        let entries = vec![
            new_test_entry("/p/proj1", "/resolved/proj1", "unique_name_1"),
            new_test_entry("/p/proj2", "/resolved/proj2", "unique_name_2"),
        ];
        let result = finder.direct_select(&entries, "unique_name_1");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap().unwrap().path,
            PathBuf::from("/resolved/proj1")
        );
    }

    #[test]
    fn test_direct_select_filename_match_ambiguous() {
        let finder = FuzzyFinder::new();
        let entries = vec![
            new_test_entry(
                "/some/common_name",
                "/resolved/some/path/common_name",
                "display1",
            ),
            new_test_entry(
                "/another/common_name",
                "/resolved/another/path/common_name",
                "display2",
            ),
        ];
        let result = finder.direct_select(&entries, "common_name");
        assert!(result.is_err());
    }
}
