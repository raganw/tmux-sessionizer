
use crate::directory_scanner::DirectoryEntry;
use std::path::PathBuf;

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

    // The main `select` method, which will integrate with the Skim library
    // to present options to the user and return the `SelectedItem`,
    // will be implemented in a subsequent task.
    //
    // Example signature for that future method:
    // pub fn select(&self, entries: Vec<DirectoryEntry>) -> Result<Option<SelectedItem>, anyhow::Error> {
    //     // Skim integration logic will go here.
    //     unimplemented!();
    // }
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
