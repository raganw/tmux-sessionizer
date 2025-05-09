
use crate::config::Config; // Add this to use the Config struct
use std::collections::HashSet; // Add this for HashSet
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

    // Helper method to process a single potential directory path
    fn process_path_candidate(
        &self,
        original_path: PathBuf,
        entries: &mut Vec<DirectoryEntry>,
        processed_resolved_paths: &mut HashSet<PathBuf>,
    ) {
        // Ensure the path is a directory. For symlinks, this checks the target.
        if !original_path.is_dir() {
            // Optionally log: tracing::debug!("Skipping non-directory: {:?}", original_path);
            return;
        }

        let resolved_path = match fs::canonicalize(&original_path) {
            Ok(p) => p,
            Err(_e) => {
                // Optionally log: tracing::warn!("Could not canonicalize path: {:?}, error: {}", original_path, e);
                return; // Skip if path cannot be canonicalized
            }
        };

        // Prevent duplicates based on canonical path
        if processed_resolved_paths.contains(&resolved_path) {
            // Optionally log: tracing::debug!("Skipping duplicate resolved path: {:?}", resolved_path);
            return;
        }

        // Apply exclusion patterns
        for pattern in &self.config.exclude_patterns {
            if pattern.is_match(original_path.to_string_lossy().as_ref())
                || pattern.is_match(resolved_path.to_string_lossy().as_ref())
            {
                // Optionally log: tracing::debug!("Skipping excluded path: {:?} (resolved: {:?}) by pattern: {}", original_path, resolved_path, pattern);
                return;
            }
        }

        // Determine display name (usually the directory name)
        let display_name = resolved_path
            .file_name()
            .map_or_else(
                || original_path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                |os_str| os_str.to_string_lossy().into_owned(),
            );

        // Skip hidden directories by default
        if display_name.starts_with('.') && display_name.len() > 1 {
            // Optionally log: tracing::debug!("Skipping hidden directory: {:?}", display_name);
            return;
        }

        let dir_entry = DirectoryEntry {
            path: original_path, // Store the path as it was found/provided
            resolved_path: resolved_path.clone(),
            display_name,
            entry_type: DirectoryType::Plain,
        };

        entries.push(dir_entry);
        processed_resolved_paths.insert(resolved_path);
    }

    pub fn scan(&self) -> Vec<DirectoryEntry> {
        let mut entries = Vec::new();
        let mut processed_resolved_paths = HashSet::new();

        // 1. Process search_paths (scan directories within them, depth 1)
        for search_path_config_entry in &self.config.search_paths {
            let search_path_base = match expand_tilde(search_path_config_entry) {
                Some(p) => p,
                None => {
                    // Optionally log: tracing::warn!("Could not expand tilde for search path: {:?}", search_path_config_entry);
                    continue;
                }
            };

            if !search_path_base.is_dir() {
                // Optionally log: tracing::warn!("Search path is not a directory or is inaccessible: {:?}", search_path_base);
                continue;
            }

            for entry_result in WalkDir::new(&search_path_base)
                .min_depth(1)
                .max_depth(1)
                .follow_links(true)
                .into_iter()
                .filter_map(Result::ok)
            {
                let original_path = entry_result.path().to_path_buf();
                self.process_path_candidate(original_path, &mut entries, &mut processed_resolved_paths);
            }
        }

        // 2. Process additional_paths (each path is a direct candidate)
        for additional_path_config_entry in &self.config.additional_paths {
            let original_path = match expand_tilde(additional_path_config_entry) {
                Some(p) => p,
                None => {
                    // Optionally log: tracing::warn!("Could not expand tilde for additional path: {:?}", additional_path_config_entry);
                    continue;
                }
            };
            // For additional_paths, the path itself is the candidate
            self.process_path_candidate(original_path, &mut entries, &mut processed_resolved_paths);
        }

        entries
    }
}
