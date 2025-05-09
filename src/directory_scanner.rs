
use crate::config::Config; // Add this to use the Config struct
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir; // Add this for directory traversal

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

// Helper function for tilde expansion
fn expand_tilde(path: &Path) -> Option<PathBuf> {
    if path.starts_with("~") {
        if let Some(home_dir) = dirs::home_dir() {
            let mut new_path = home_dir;
            if path.components().count() > 1 { // Check if there's anything after ~
                 // Strip "~/" prefix and join the rest
                new_path.push(path.strip_prefix("~").unwrap().strip_prefix("/").unwrap_or_else(|_| path.strip_prefix("~").unwrap()));
            }
            Some(new_path)
        } else {
            None // Home directory could not be determined
        }
    } else {
        Some(path.to_path_buf()) // Path does not start with tilde, return as is
    }
}


pub struct DirectoryScanner<'a> {
    config: &'a Config,
}

impl<'a> DirectoryScanner<'a> {
    pub fn new(config: &'a Config) -> Self {
        Self { config }
    }

    pub fn scan(&self) -> Vec<DirectoryEntry> {
        let mut entries = Vec::new();

        for search_path_config_entry in &self.config.search_paths {
            let search_path = match expand_tilde(search_path_config_entry) {
                Some(p) => p,
                None => {
                    // Optionally log: eprintln!("Warning: Could not expand tilde for path: {:?}", search_path_config_entry);
                    continue;
                }
            };

            if !search_path.is_dir() {
                // Optionally log: eprintln!("Warning: Search path is not a directory or is inaccessible: {:?}", search_path);
                continue;
            }

            for entry_result in WalkDir::new(&search_path)
                .min_depth(1) // Do not include the search_path itself
                .max_depth(1) // Only direct children of search_path
                .follow_links(true) // Follow symlinks to their targets
                .into_iter()
                .filter_map(Result::ok) // Ignore errors during iteration (e.g., permission denied for a subdir)
            {
                let original_path = entry_result.path().to_path_buf();

                // We are interested in directories. If `follow_links` is true,
                // `is_dir()` on the path from WalkDir entry checks the target of a symlink.
                if !original_path.is_dir() {
                    continue;
                }

                let resolved_path = match fs::canonicalize(&original_path) {
                    Ok(p) => p,
                    Err(_) => {
                        // Optionally log: eprintln!("Warning: Could not canonicalize path: {:?}", original_path);
                        continue; // Skip if path cannot be canonicalized (e.g. broken symlink target)
                    }
                };

                // Apply exclusion patterns
                let mut excluded = false;
                for pattern in &self.config.exclude_patterns {
                    if pattern.is_match(original_path.to_string_lossy().as_ref())
                        || pattern.is_match(resolved_path.to_string_lossy().as_ref())
                    {
                        excluded = true;
                        break;
                    }
                }

                if excluded {
                    continue;
                }

                // Determine display name (usually the directory name)
                let display_name = resolved_path
                    .file_name()
                    .map_or_else(
                        || original_path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                        |os_str| os_str.to_string_lossy().into_owned(),
                    );
                
                if display_name.starts_with('.') && display_name.len() > 1 { // Basic check for hidden directories like .git, .config etc.
                    continue; // Skip hidden directories by default, unless explicitly configured otherwise later
                }


                let dir_entry = DirectoryEntry {
                    path: original_path.clone(),
                    resolved_path: resolved_path.clone(),
                    display_name,
                    entry_type: DirectoryType::Plain,
                };
                entries.push(dir_entry);
            }
        }
        entries
    }
}
