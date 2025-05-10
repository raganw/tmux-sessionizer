
use crate::directory_scanner::DirectoryEntry;
use anyhow::{anyhow, Context, Result}; // Add anyhow for error handling
use skim::prelude::*; // Add skim prelude
use std::io::Cursor; // To create a BufRead from String for Skim
use std::path::PathBuf;
use tracing::debug; // For logging

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
    /// * `Err(anyhow::Error)` if an error occurs during the Skim process or parsing the selection.
    pub fn select(&self, entries: Vec<DirectoryEntry>) -> Result<Option<SelectedItem>> {
        if entries.is_empty() {
            debug!("No entries provided to fuzzy finder, returning None.");
            return Ok(None);
        }

        let skim_input = self.prepare_skim_input(&entries);
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
            .height(Some("50%"))
            .multi(false) // Single selection mode
            .prompt(Some("Select project: "))
            // .header(Some("Choose a directory:")) // Alternative to prompt
            // .preview(Some("")) // Enable preview window, command can be set
            // .delimiter(Some("\t")) // If Skim needs to parse fields internally
            .build()
            .map_err(|e| anyhow!("Failed to build Skim options: {}", e))?;

        // Create an item reader from the prepared input string
        let item_reader = SkimItemReader::default();
        let items = item_reader.of_bufread(Cursor::new(skim_input));

        // Run Skim and process the output
        let skim_output = Skim::run_with(&options, Some(items))
            .context("Skim execution failed or was cancelled by user initially")?;
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
        let selected_skim_item = selected_items
            .first()
            .ok_or_else(|| anyhow!("Skim reported selection but no items found in selected_items list"))?;

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
            Err(anyhow!(
                "Selected line from Skim has unexpected format (expected 'display\\tpath'): '{}'",
                selected_line
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directory_scanner::DirectoryType; // For creating test DirectoryEntry

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
}
