use crate::config::Config; // Add this to use the Config struct
use crate::git_repository_handler::{
    self, is_git_repository, list_linked_worktrees, Worktree,
};
use git2::Repository; // For repo.is_worktree()
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

    // Add a helper to add worktree entries to avoid repetition
    fn add_worktree_entry(
        &self,
        wt_path: PathBuf, // This should be the resolved path of the worktree
        main_repo_resolved_path: &Path,
        worktree_name_opt: Option<String>, // Optional: name from git worktree list
        entries: &mut Vec<DirectoryEntry>,
        processed_resolved_paths: &mut HashSet<PathBuf>,
    ) {
        if processed_resolved_paths.contains(&wt_path) {
            debug!(path = %wt_path.display(), "Worktree path already processed, skipping");
            return;
        }

        // The display name for a worktree will be handled more specifically in task 5.
        // For now, use its directory name or the provided worktree name.
        let display_name = worktree_name_opt.unwrap_or_else(|| {
            wt_path.file_name().unwrap_or_default().to_string_lossy().into_owned()
        });

        debug!(path = %wt_path.display(), main_repo = %main_repo_resolved_path.display(), name = %display_name, "Adding Git worktree entry");
        let worktree_entry = DirectoryEntry {
            path: wt_path.clone(), // Original path is same as resolved for these directly identified worktrees
            resolved_path: wt_path.clone(),
            display_name,
            entry_type: DirectoryType::GitWorktree {
                main_worktree_path: main_repo_resolved_path.to_path_buf(),
            },
            parent_path: Some(main_repo_resolved_path.to_path_buf()),
        };
        entries.push(worktree_entry);
        processed_resolved_paths.insert(wt_path);
    }


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
                return;
            }
        };
        debug!(original = %original_path.display(), resolved = %resolved_path.display(), "Path resolved");

        if processed_resolved_paths.contains(&resolved_path) {
            debug!(path = %resolved_path.display(), "Skipping duplicate resolved path");
            return;
        }

        for pattern in &self.config.exclude_patterns {
            if pattern.is_match(original_path.to_string_lossy().as_ref())
                || pattern.is_match(resolved_path.to_string_lossy().as_ref()) {
                debug!(path = %resolved_path.display(), pattern = %pattern, "Skipping excluded path");
                return;
            }
        }
        
        let display_name_candidate = resolved_path
            .file_name()
            .map_or_else(
                || original_path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                |os_str| os_str.to_string_lossy().into_owned(),
            );

        if display_name_candidate.starts_with('.') && display_name_candidate.len() > 1 && display_name_candidate != ".git" {
             // Allow .git as it might be a bare repo name, but generally skip hidden dirs.
            debug!(name = %display_name_candidate, path = %resolved_path.display(), "Skipping hidden directory");
            return;
        }

        // Git repository detection
        if is_git_repository(&resolved_path) {
            match Repository::open(&resolved_path) {
                Ok(repo) => {
                    if repo.is_worktree() {
                        // This path is a Git worktree
                        match git_repository_handler::get_main_repository_path(&resolved_path) {
                            Ok(main_repo_path) => {
                                self.add_worktree_entry(
                                    resolved_path.clone(), // worktree's own path
                                    &main_repo_path,       // path to its main repo
                                    None, // name will be basename of resolved_path for now
                                    entries,
                                    processed_resolved_paths,
                                );
                            }
                            Err(e) => {
                                warn!(path = %resolved_path.display(), error = %e, "Failed to get main repository path for worktree, treating as plain directory");
                                // Fallback to adding as plain directory if main repo path fails
                                self.add_plain_directory_entry(original_path, resolved_path, display_name_candidate, entries, processed_resolved_paths);
                            }
                        }
                    } else {
                        // This path is a main Git repository (standard or bare)
                        debug!(path = %resolved_path.display(), name = %display_name_candidate, "Adding Git repository entry");
                        let repo_entry = DirectoryEntry {
                            path: original_path,
                            resolved_path: resolved_path.clone(),
                            display_name: display_name_candidate.clone(), // Use candidate, final formatting in task 5
                            entry_type: DirectoryType::GitRepository,
                            parent_path: None,
                        };
                        entries.push(repo_entry);
                        processed_resolved_paths.insert(resolved_path.clone());

                        // Now, find and add its linked worktrees
                        match list_linked_worktrees(&resolved_path) {
                            Ok(linked_worktrees) => {
                                debug!(repo_path = %resolved_path.display(), count = linked_worktrees.len(), "Found linked worktrees");
                                for worktree_info in linked_worktrees {
                                    // Ensure worktree_info.path is canonicalized for consistent checking
                                    match fs::canonicalize(&worktree_info.path) {
                                        Ok(canonical_wt_path) => {
                                             self.add_worktree_entry(
                                                canonical_wt_path,
                                                &resolved_path, // The current repo is the main repo for these worktrees
                                                Some(worktree_info.name),
                                                entries,
                                                processed_resolved_paths,
                                            );
                                        }
                                        Err(e) => {
                                            warn!(wt_path = %worktree_info.path.display(), error = %e, "Could not canonicalize linked worktree path, skipping");
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                warn!(repo_path = %resolved_path.display(), error = %e, "Failed to list linked worktrees");
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(path = %resolved_path.display(), error = %e, "Failed to open path as Git repository despite initial check, treating as plain");
                    self.add_plain_directory_entry(original_path, resolved_path, display_name_candidate, entries, processed_resolved_paths);
                }
            }
        } else {
            // Not a Git repository, add as plain directory
            self.add_plain_directory_entry(original_path, resolved_path, display_name_candidate, entries, processed_resolved_paths);
        }
    }
    
    // Helper function to add plain directory entries
    fn add_plain_directory_entry(
        &self,
        original_path: PathBuf,
        resolved_path: PathBuf,
        display_name: String,
        entries: &mut Vec<DirectoryEntry>,
        processed_resolved_paths: &mut HashSet<PathBuf>,
    ) {
        // This check is technically redundant if called after the main check in process_path_candidate,
        // but good for a standalone helper.
        if processed_resolved_paths.contains(&resolved_path) {
            debug!(path = %resolved_path.display(), "Plain directory path already processed, skipping");
            return;
        }
        debug!(name = %display_name, path = %resolved_path.display(), "Adding plain directory entry");
        let dir_entry = DirectoryEntry {
            path: original_path,
            resolved_path: resolved_path.clone(),
            display_name,
            entry_type: DirectoryType::Plain,
            parent_path: None,
        };
        entries.push(dir_entry);
        processed_resolved_paths.insert(resolved_path);
    }


    pub fn scan(&self) -> Vec<DirectoryEntry> {
        // ... (scan method largely remains the same, calling process_path_candidate)
        // Ensure you are using the updated process_path_candidate
        let scan_span = span!(Level::INFO, "directory_scan");
        let _enter = scan_span.enter();

        info!("Starting directory scan");
        let mut entries = Vec::new();
        let mut processed_resolved_paths = HashSet::new(); // Used to avoid duplicates

        debug!(search_paths = ?self.config.search_paths, "Processing search paths");
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
                .follow_links(true) // Important: follow symlinks to find actual directories
                .into_iter()
                .filter_map(|e| {
                    if let Err(ref err_val) = e {
                        // Log errors from WalkDir, e.g. permission denied
                        warn!(path = ?err_val.path(), error = %err_val, "Error walking directory child");
                    }
                    e.ok()
                })
            {
                let original_path = entry_result.path().to_path_buf();
                // process_path_candidate will check if it's a directory and handle symlinks via canonicalize
                self.process_path_candidate(original_path, &mut entries, &mut processed_resolved_paths);
            }
        }

        debug!(additional_paths = ?self.config.additional_paths, "Processing additional paths");
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
            self.process_path_candidate(original_path, &mut entries, &mut processed_resolved_paths);
        }

        info!(count = entries.len(), "Directory scan complete");
        debug!(final_entries = ?entries, "Final list of directory entries");
        entries
    }
}
