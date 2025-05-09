
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum DirectoryType {
    Plain,
    // GitRepository, // To be added later
    // GitWorktree { main_worktree: PathBuf }, // To be added later
    // GitWorktreeContainer, // To be added later
}

#[derive(Debug, Clone, PartialEq)]
pub struct DirectoryEntry {
    pub path: PathBuf,
    pub resolved_path: PathBuf,
    pub display_name: String,
    pub entry_type: DirectoryType,
    // pub parent_path: Option<PathBuf>, // For worktrees, to be added later
}
