use crate::config::Config; // Add this to use the Config struct
use crate::git_repository_handler::{
    self, is_git_repository, list_linked_worktrees,
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
        original_wt_path: PathBuf, // The path as found by WalkDir or from git config
        resolved_wt_path: PathBuf, // The canonicalized path of the worktree
        main_repo_resolved_path: &Path,
        _worktree_name_opt: Option<String>, // Name from git worktree list, currently not used for display name per spec
        entries: &mut Vec<DirectoryEntry>,
        processed_resolved_paths: &mut HashSet<PathBuf>,
    ) {
        if processed_resolved_paths.contains(&resolved_wt_path) {
            debug!(path = %resolved_wt_path.display(), "Worktree path already processed, skipping");
            return;
        }

        let worktree_basename = resolved_wt_path.file_name().unwrap_or_default().to_string_lossy();
        let parent_basename = main_repo_resolved_path.file_name().unwrap_or_default().to_string_lossy();
        let display_name = format!("[{}] {}", parent_basename, worktree_basename);

        debug!(path = %resolved_wt_path.display(), main_repo = %main_repo_resolved_path.display(), name = %display_name, "Adding Git worktree entry");
        let worktree_entry = DirectoryEntry {
            path: original_wt_path, // Use the original path
            resolved_path: resolved_wt_path.clone(),
            display_name,
            entry_type: DirectoryType::GitWorktree {
                main_worktree_path: main_repo_resolved_path.to_path_buf(),
            },
            parent_path: Some(main_repo_resolved_path.to_path_buf()),
        };
        entries.push(worktree_entry);
        processed_resolved_paths.insert(resolved_wt_path);
    }


    fn process_path_candidate(
        &self,
        original_path: PathBuf, // Path as found by WalkDir
        entries: &mut Vec<DirectoryEntry>,
        processed_resolved_paths: &mut HashSet<PathBuf>,
    ) {
        let candidate_span = span!(Level::DEBUG, "process_path_candidate", path = %original_path.display());
        let _enter = candidate_span.enter();

        // if !original_path.is_dir() { // Check on original_path before canonicalization for symlinks to files
            // If original_path is a symlink, is_dir() checks the target.
            // This check is more about whether the entry from WalkDir itself is a directory.
            // However, WalkDir should only yield directories if configured, or we filter by entry.file_type().is_dir()
            // fs::canonicalize below will fail for symlinks to non-existent targets or non-directories.
            // For now, let's rely on canonicalize error handling and the is_dir check on resolved_path if needed.
            // The existing check `if !original_path.is_dir()` might be problematic if original_path is a symlink
            // and follow_links is false for WalkDir (but it's true).
            // Let's assume WalkDir gives us something that is, or points to, a directory.
            // Canonicalization will give the real path.
        // }

        let resolved_path = match fs::canonicalize(&original_path) {
            Ok(p) => p,
            Err(e) => {
                warn!(original_path = %original_path.display(), error = %e, "Could not canonicalize path, skipping");
                return;
            }
        };

        // After canonicalization, check if it's actually a directory.
        // This handles cases where original_path might be a symlink to a file.
        if !resolved_path.is_dir() {
            debug!(original_path = %original_path.display(), resolved_path = %resolved_path.display(), "Skipping as resolved path is not a directory");
            return;
        }
        
        debug!(original = %original_path.display(), resolved = %resolved_path.display(), "Path resolved");

        if processed_resolved_paths.contains(&resolved_path) {
            debug!(path = %resolved_path.display(), "Skipping duplicate resolved path");
            return;
        }

        for pattern in &self.config.exclude_patterns {
            if pattern.is_match(original_path.to_string_lossy().as_ref()) // Check original path against exclusions
                || pattern.is_match(resolved_path.to_string_lossy().as_ref()) { // Check resolved path
                debug!(path = %resolved_path.display(), pattern = %pattern, "Skipping excluded path");
                return;
            }
        }
        
        let basename_of_resolved_path = resolved_path
            .file_name()
            .map_or_else(
                // Fallback to original_path's basename if resolved_path has no filename (e.g. "/")
                || original_path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                |os_str| os_str.to_string_lossy().into_owned(),
            );

        if basename_of_resolved_path.starts_with('.') && basename_of_resolved_path.len() > 1 && basename_of_resolved_path != ".git" {
            debug!(name = %basename_of_resolved_path, path = %resolved_path.display(), "Skipping hidden directory");
            return;
        }

        if is_git_repository(&resolved_path) {
            match Repository::open(&resolved_path) {
                Ok(repo) => {
                    if repo.is_worktree() {
                        match git_repository_handler::get_main_repository_path(&resolved_path) {
                            Ok(main_repo_path) => {
                                self.add_worktree_entry(
                                    original_path.clone(), // Pass original path
                                    resolved_path.clone(), // Pass resolved path
                                    &main_repo_path,
                                    None, // worktree_name_opt, basename will be derived from resolved_path
                                    entries,
                                    processed_resolved_paths,
                                );
                            }
                            Err(e) => {
                                warn!(path = %resolved_path.display(), error = %e, "Failed to get main repository path for worktree, treating as plain directory");
                                self.add_plain_directory_entry(original_path, resolved_path, basename_of_resolved_path, entries, processed_resolved_paths);
                            }
                        }
                    } else { // Main Git repository (standard or bare)
                        debug!(path = %resolved_path.display(), name = %basename_of_resolved_path, "Adding Git repository entry");
                        let repo_entry = DirectoryEntry {
                            path: original_path.clone(), // Use original path
                            resolved_path: resolved_path.clone(),
                            display_name: basename_of_resolved_path.clone(), // Basename of resolved_path
                            entry_type: DirectoryType::GitRepository,
                            parent_path: None,
                        };
                        entries.push(repo_entry);
                        processed_resolved_paths.insert(resolved_path.clone());

                        match list_linked_worktrees(&resolved_path) {
                            Ok(linked_worktrees) => {
                                debug!(repo_path = %resolved_path.display(), count = linked_worktrees.len(), "Found linked worktrees");
                                for worktree_info in linked_worktrees {
                                    let wt_path_from_git = worktree_info.path; // Path as stored in git
                                    match fs::canonicalize(&wt_path_from_git) {
                                        Ok(canonical_wt_path) => {
                                            if !canonical_wt_path.is_dir() {
                                                warn!(wt_path = %wt_path_from_git.display(), resolved_wt_path = %canonical_wt_path.display(), "Linked worktree path is not a directory, skipping");
                                                continue;
                                            }
                                             self.add_worktree_entry(
                                                wt_path_from_git.clone(), // Original is path from git
                                                canonical_wt_path,        // Resolved path
                                                &resolved_path, // The current repo is the main repo
                                                Some(worktree_info.name), // Name from git, not used for display but passed
                                                entries,
                                                processed_resolved_paths,
                                            );
                                        }
                                        Err(e) => {
                                            warn!(wt_path = %wt_path_from_git.display(), error = %e, "Could not canonicalize linked worktree path, skipping");
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
                    // Before adding as plain, check if it's a worktree container
                    if self.check_if_worktree_container(&resolved_path) {
                        debug!(path = %resolved_path.display(), "Identified as a Git worktree container (after failing to open as repo), skipping");
                        return;
                    }
                    self.add_plain_directory_entry(original_path, resolved_path, basename_of_resolved_path, entries, processed_resolved_paths);
                }
            }
        } else { // Not a Git repository (is_git_repository(&resolved_path) returned false)
            // Check if it's a GitWorktreeContainer before treating as plain
            if self.check_if_worktree_container(&resolved_path) {
                debug!(path = %resolved_path.display(), "Identified as a Git worktree container, skipping");
                // Do not add to entries, effectively excluding it.
                // We also don't add to processed_resolved_paths here, as it's not an "entry".
                // If this path were encountered again via a different original_path (e.g. another symlink),
                // it would be re-evaluated, which is fine.
                return;
            } else {
                // If not a container, then it's a plain directory
                self.add_plain_directory_entry(original_path, resolved_path, basename_of_resolved_path, entries, processed_resolved_paths);
            }
        }
    }
    
    // Helper function to add plain directory entries
    fn add_plain_directory_entry(
        &self,
        original_path: PathBuf,
        resolved_path: PathBuf,
        display_name: String, // This is already basename of resolved_path
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
            path: original_path, // Use the original path
            resolved_path: resolved_path.clone(),
            display_name, // Basename of resolved_path
            entry_type: DirectoryType::Plain,
            parent_path: None,
        };
        entries.push(dir_entry);
        processed_resolved_paths.insert(resolved_path);
    }

    /// Checks if the given path is a "worktree container".
    /// A directory is considered a worktree container if all of its direct children are
    /// directories, each of those is a Git worktree, and all those worktrees belong
    /// to the same main repository. It must also contain at least one such worktree
    /// and no other files or non-qualifying directories at its top level.
    ///
    /// Returns `true` if it's a worktree container, `false` otherwise.
    fn check_if_worktree_container(&self, path_to_check: &Path) -> bool {
        let container_check_span = span!(Level::DEBUG, "check_if_worktree_container", path = %path_to_check.display());
        let _enter = container_check_span.enter();

        let mut worktree_children_count = 0;
        let mut first_main_repo_path: Option<PathBuf> = None;
        let mut all_children_are_qualifying_worktrees = true;

        match fs::read_dir(path_to_check) {
            Ok(entries) => {
                for entry_result in entries {
                    match entry_result {
                        Ok(entry) => {
                            let child_path = entry.path();
                            let child_file_type = match entry.file_type() {
                                Ok(ft) => ft,
                                Err(e) => {
                                    warn!(child = %child_path.display(), error = %e, "Could not get file type for child, assuming not a qualifying worktree container child.");
                                    all_children_are_qualifying_worktrees = false;
                                    break;
                                }
                            };

                            // If it's a file (not a directory or symlink to dir), then not a container.
                            // Symlinks to files will also fail canonical_child_path.is_dir() check later.
                            if child_file_type.is_file() {
                                debug!(child = %child_path.display(), "Child is a file, parent not a worktree container.");
                                all_children_are_qualifying_worktrees = false;
                                break;
                            }

                            // If it's a directory or symlink (which WalkDir would follow, but read_dir doesn't by default)
                            // We need to check if it resolves to a directory and is a worktree.
                            if child_file_type.is_dir() || child_file_type.is_symlink() {
                                match fs::canonicalize(&child_path) {
                                    Ok(canonical_child_path) => {
                                        if !canonical_child_path.is_dir() {
                                            // Symlink pointed to a file or non-existent, or other issue.
                                            debug!(child = %child_path.display(), resolved = %canonical_child_path.display(), "Child resolved to non-directory, parent not a worktree container.");
                                            all_children_are_qualifying_worktrees = false;
                                            break;
                                        }

                                        // It's a directory, proceed to check if it's a worktree
                                        match Repository::open(&canonical_child_path) {
                                            Ok(repo) => {
                                                if repo.is_worktree() {
                                                    match git_repository_handler::get_main_repository_path(&canonical_child_path) {
                                                        Ok(main_repo_path) => {
                                                            if first_main_repo_path.is_none() {
                                                                first_main_repo_path = Some(main_repo_path.clone());
                                                                debug!(child_worktree = %canonical_child_path.display(), main_repo = %main_repo_path.display(), "First worktree found, setting common main repo path.");
                                                            } else if first_main_repo_path != Some(main_repo_path.clone()) {
                                                                debug!(child_worktree = %canonical_child_path.display(), main_repo = %main_repo_path.display(), expected_main_repo = ?first_main_repo_path, "Worktree belongs to a different main repo, parent not a container.");
                                                                all_children_are_qualifying_worktrees = false;
                                                                break;
                                                            }
                                                            // If main repo paths match, increment count
                                                            worktree_children_count += 1;
                                                        }
                                                        Err(e) => { // Failed to get main repo path
                                                            warn!(child_worktree = %canonical_child_path.display(), error = %e, "Failed to get main repository path for worktree child.");
                                                            all_children_are_qualifying_worktrees = false;
                                                            break;
                                                        }
                                                    }
                                                } else { // Child is a Git repo, but not a worktree
                                                    debug!(child = %canonical_child_path.display(), "Child is a Git repository but not a worktree, parent not a container.");
                                                    all_children_are_qualifying_worktrees = false;
                                                    break;
                                                }
                                            }
                                            Err(_) => { // Child is not a Git repository at all
                                                debug!(child = %canonical_child_path.display(), "Child is not a Git repository, parent not a worktree container.");
                                                all_children_are_qualifying_worktrees = false;
                                                break;
                                            }
                                        }
                                    }
                                    Err(e) => { // Failed to canonicalize child path
                                        warn!(child = %child_path.display(), error = %e, "Could not canonicalize child path.");
                                        all_children_are_qualifying_worktrees = false;
                                        break;
                                    }
                                }
                            } else {
                                // Neither file, dir, nor symlink. Should not happen with fs::read_dir normally.
                                debug!(child = %child_path.display(), "Child is of unknown type, parent not a worktree container.");
                                all_children_are_qualifying_worktrees = false;
                                break;
                            }
                        }
                        Err(e) => { // Error reading a specific directory entry
                            warn!(path = %path_to_check.display(), error = %e, "Error iterating directory entry.");
                            all_children_are_qualifying_worktrees = false;
                            break;
                        }
                    }
                }
            }
            Err(e) => { // Error reading the parent directory itself
                warn!(path = %path_to_check.display(), error = %e, "Could not read directory to check for worktree container status.");
                return false; // Cannot determine, so assume not a container
            }
        }

        let is_container = all_children_are_qualifying_worktrees && worktree_children_count > 0 && first_main_repo_path.is_some();
        if is_container {
            debug!(path = %path_to_check.display(), common_main_repo = ?first_main_repo_path, worktree_count = worktree_children_count, "Path IS a worktree container.");
        } else {
            debug!(path = %path_to_check.display(), all_children_ok = all_children_are_qualifying_worktrees, worktree_count = worktree_children_count, main_repo_found = first_main_repo_path.is_some(), "Path is NOT a worktree container.");
        }
        is_container
    }

    // scan function remains the same as it calls process_path_candidate
    // ... (scan function definition) ...
    pub fn scan(&self) -> Vec<DirectoryEntry> {
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
                        let io_error_string = err_val
                            .io_error()
                            .map_or_else(|| "N/A".to_string(), |ioe| ioe.to_string());
                        warn!(path = ?err_val.path(), error = %io_error_string, "Error walking directory child");
                    }
                    e.ok()
                })
            {
                // entry_result.path() is already resolved if follow_links is true,
                // but we pass it as original_path to process_path_candidate, which then canonicalizes it again.
                // This is slightly redundant but ensures canonicalization.
                // A more optimized way would be to use entry_result.path() as potentially pre-resolved.
                // However, fs::canonicalize is robust.
                let path_from_walkdir = entry_result.path().to_path_buf();
                self.process_path_candidate(path_from_walkdir, &mut entries, &mut processed_resolved_paths);
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
