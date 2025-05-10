
use crate::config::Config; // Add this to use the Config struct
use std::collections::HashSet; // Add this for HashSet
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn, span, Level}; // Add tracing imports
use walkdir::WalkDir; // Add this for directory traversal

#[derive(Debug, Clone, PartialEq)]
pub enum DirectoryType {
    Plain,
    GitRepository,
    GitWorktree { main_worktree_path: PathBuf }, // Path to the main .git dir or main worktree's root
    GitWorktreeContainer, // A directory that primarily contains worktrees of a single main repo
}

#[derive(Debug, Clone, PartialEq)]
pub struct DirectoryEntry {
    pub path: PathBuf,
    pub resolved_path: PathBuf,
    pub display_name: String,
    pub entry_type: DirectoryType,
    pub parent_path: Option<PathBuf>, // For worktrees, to reference their main repo's path
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
        let candidate_span = span!(Level::DEBUG, "process_path_candidate", path = %original_path.display());
        let _enter = candidate_span.enter();

        if !original_path.is_dir() {
            debug!(path = %original_path.display(), "Skipping non-directory");
            return;
        }

        let resolved_path = match fs::canonicalize(&original_path) {
            Ok(p) => p,
            Err(e) => {
                warn!(path = %original_path.display(), error = %e, "Could not canonicalize path, skipping");
                return; // Skip if path cannot be canonicalized
            }
        };
        debug!(original = %original_path.display(), resolved = %resolved_path.display(), "Path resolved");

        // Prevent duplicates based on canonical path
        if processed_resolved_paths.contains(&resolved_path) {
            debug!(path = %resolved_path.display(), "Skipping duplicate resolved path");
            return;
        }

        // Apply exclusion patterns
        for pattern in &self.config.exclude_patterns {
            if pattern.is_match(original_path.to_string_lossy().as_ref()) {
                debug!(path = %original_path.display(), pattern = %pattern, "Skipping excluded original path");
                return;
            }
            if pattern.is_match(resolved_path.to_string_lossy().as_ref()) {
                debug!(path = %resolved_path.display(), pattern = %pattern, "Skipping excluded resolved path");
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
            debug!(name = %display_name, path = %resolved_path.display(), "Skipping hidden directory");
            return;
        }

        debug!(name = %display_name, path = %resolved_path.display(), "Adding directory entry");
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
        let scan_span = span!(Level::INFO, "directory_scan");
        let _enter = scan_span.enter();

        info!("Starting directory scan");
        let mut entries = Vec::new();
        let mut processed_resolved_paths = HashSet::new();

        debug!(search_paths = ?self.config.search_paths, "Processing search paths");
        // 1. Process search_paths (scan directories within them, depth 1)
        for search_path_config_entry in &self.config.search_paths {
            let path_span = span!(Level::DEBUG, "process_search_root", config_path = %search_path_config_entry.display());
            let _path_enter = path_span.enter();

            let search_path_base = match expand_tilde(search_path_config_entry) {
                Some(p) => p,
                None => {
                    warn!(path = %search_path_config_entry.display(), "Could not expand tilde for search path, skipping");
                    continue;
                }
            };
            debug!(expanded_path = %search_path_base.display(), "Expanded search path");

            if !search_path_base.is_dir() {
                warn!(path = %search_path_base.display(), "Search path is not a directory or is inaccessible, skipping");
                continue;
            }

            debug!(path = %search_path_base.display(), "Iterating direct children");
            for entry_result in WalkDir::new(&search_path_base)
                .min_depth(1)
                .max_depth(1)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| {
                    if e.is_err() {
                        warn!(error = ?e.as_ref().err(), "Error iterating directory child");
                    }
                    e.ok()
                })
            {
                let original_path = entry_result.path().to_path_buf();
                debug!(found_path = %original_path.display(), "Found potential directory in search path");
                self.process_path_candidate(original_path, &mut entries, &mut processed_resolved_paths);
            }
        }

        debug!(additional_paths = ?self.config.additional_paths, "Processing additional paths");
        // 2. Process additional_paths (each path is a direct candidate)
        for additional_path_config_entry in &self.config.additional_paths {
             let path_span = span!(Level::DEBUG, "process_additional_path", config_path = %additional_path_config_entry.display());
            let _path_enter = path_span.enter();

            let original_path = match expand_tilde(additional_path_config_entry) {
                Some(p) => p,
                None => {
                    warn!(path = %additional_path_config_entry.display(), "Could not expand tilde for additional path, skipping");
                    continue;
                }
            };
            debug!(expanded_path = %original_path.display(), "Expanded additional path");
            // For additional_paths, the path itself is the candidate
            self.process_path_candidate(original_path, &mut entries, &mut processed_resolved_paths);
        }

        info!(count = entries.len(), "Directory scan complete");
        debug!(final_entries = ?entries, "Final list of directory entries");
        entries
    }
}
