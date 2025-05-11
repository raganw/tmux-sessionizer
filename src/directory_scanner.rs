use crate::config::Config; // Add this to use the Config struct
use crate::git_repository_handler::{
    self, is_git_repository, list_linked_worktrees,
};
use git2::Repository; // For repo.is_worktree()
use std::collections::HashSet; // Add this for HashSet
use std::ffi::OsStr; // For comparing OsStr with ".git"
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn, span, Level}; // Add tracing imports
use walkdir::WalkDir; // Add this for directory traversal

#[derive(Debug, Clone, PartialEq)]
pub enum DirectoryType {
    Plain,
    GitRepository,
    GitWorktree { main_worktree_path: PathBuf }, // Path to the main .git dir or main worktree's root
    #[allow(dead_code)] // This variant is used conceptually for exclusion, not direct construction of entries.
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

    /// Checks if the given path, which is known to be a bare Git repository,
    /// also serves as a container for its own worktrees.
    ///
    /// Under the relaxed definition, this is true if the `container_candidate_path` has at least one
    /// direct child subdirectory that is a Git worktree belonging to this specific bare repository.
    /// Other files or non-qualifying subdirectories within `container_candidate_path` do not prevent it
    /// from being considered a container, as long as the worktree condition is met.
    ///
    /// The `bare_repo` argument is the `Repository` object associated with `container_candidate_path`
    /// (which means `bare_repo.path()` gives the path to the actual bare Git directory).
    ///
    /// Returns `true` if it contains at least one qualifying worktree child, `false` otherwise.
    fn is_bare_repo_worktree_exclusive_container(&self, container_candidate_path: &Path, bare_repo: &Repository) -> bool {
        let container_check_span = span!(Level::DEBUG, "is_bare_repo_worktree_exclusive_container", path = %container_candidate_path.display());
        let _enter = container_check_span.enter();
        // Removed: all_children_are_qualifying_or_ignored flag. We only care about finding at least one worktree.

        let mut worktree_children_count = 0;

        let canonical_bare_repo_dotgit_path = match fs::canonicalize(bare_repo.path()) {
            Ok(p) => p,
            Err(e) => {
                warn!(path = %bare_repo.path().display(), error = %e, "Failed to canonicalize bare repo .git path, cannot reliably check for container status.");
                return false; // Cannot determine, assume not a container
            }
        };
        debug!(path = %container_candidate_path.display(), bare_repo_dotgit_path = %canonical_bare_repo_dotgit_path.display(), "Checking for bare repo exclusive container");

        match fs::read_dir(container_candidate_path) {
            Ok(dir_entries) => {
                for entry_result in dir_entries {
                    match entry_result {
                        Ok(entry) => {
                            let child_path = entry.path();
                            let child_name = child_path.file_name().unwrap_or_default();

                            // 1. Ignore the .git file/directory in the container_candidate_path itself.
                            // This is what makes container_candidate_path a repo.
                            if child_name == OsStr::new(".git") {
                                debug!(child = %child_path.display(), "Ignoring .git entry in container.");
                                continue;
                            }

                            let canonical_child_path = match fs::canonicalize(&child_path) {
                                Ok(p) => p,
                                Err(e) => {
                                    warn!(child = %child_path.display(), error = %e, "Could not canonicalize child path, assuming not a qualifying worktree container child.");
                                    // Relaxed: Continue checking other children
                                    continue;
                                }
                            };

                            // 2. Ignore the directory that is the actual bare repo's .git directory, if it's a direct child.
                            // (e.g. container_candidate_path/actual_bare.git/ where bare_repo.path() points to actual_bare.git/)
                            if canonical_child_path == canonical_bare_repo_dotgit_path {
                                debug!(child = %child_path.display(), "Ignoring actual bare repo .git directory child.");
                                continue;
                            }

                            let child_file_type = match entry.file_type() {
                                Ok(ft) => ft,
                                Err(e) => {
                                    warn!(child = %child_path.display(), error = %e, "Could not get file type for child, assuming not a qualifying worktree container child.");
                                    // Relaxed: Continue checking other children
                                    continue;
                                }
                            };

                            if child_file_type.is_file() {
                                debug!(child = %child_path.display(), "Child is a file, not a worktree. Continuing search for worktrees.");
                                // Relaxed: This file is permissible. Continue checking other entries.
                                continue;
                            }

                            if child_file_type.is_dir() { // Symlinks to dirs are fine if they are worktrees
                                match Repository::open(&canonical_child_path) {
                                    Ok(child_repo) => {
                                        if child_repo.is_worktree() {
                                            match git_repository_handler::get_main_repository_path(&canonical_child_path) {
                                                Ok(wt_main_repo_path) => {
                                                    match fs::canonicalize(&wt_main_repo_path) {
                                                        Ok(canonical_wt_main_repo_path) => {
                                                            if canonical_wt_main_repo_path == canonical_bare_repo_dotgit_path {
                                                                debug!(child_worktree = %canonical_child_path.display(), "Child is a qualifying worktree of this bare repo.");
                                                                worktree_children_count += 1;
                                                            } else {
                                                                debug!(child_worktree = %canonical_child_path.display(), main_repo = %canonical_wt_main_repo_path.display(), expected_main_repo = %canonical_bare_repo_dotgit_path.display(), "Child worktree belongs to a different main repo.");
                                                                // Relaxed: Continue checking other children
                                                                continue;
                                                            }
                                                        }
                                                        Err(e) => {
                                                            warn!(wt_main_repo_path = %wt_main_repo_path.display(), error = %e, "Failed to canonicalize worktree's main repo path.");
                                                            // Relaxed: Continue checking other children
                                                            continue;
                                                        }
                                                    }
                                                }
                                                Err(e) => { // Failed to get main repo path for child worktree
                                                    warn!(child_worktree = %canonical_child_path.display(), error = %e, "Failed to get main repository path for worktree child.");
                                                    // Relaxed: Continue checking other children
                                                    continue;
                                                }
                                            }
                                        } else { // Child is a Git repo, but not a worktree
                                            debug!(child = %canonical_child_path.display(), "Child is a Git repository but not a worktree.");
                                            // Relaxed: Continue checking other children
                                            continue;
                                        }
                                    }
                                    Err(_) => { // Child is not a Git repository at all
                                        debug!(child = %canonical_child_path.display(), "Child is not a Git repository.");
                                        // Relaxed: Continue checking other children
                                        continue;
                                    }
                                }
                            } else { // Neither file, dir, nor symlink to dir (symlink to file would have been caught by child_file_type.is_file() if symlink followed)
                                debug!(child = %child_path.display(), "Child is of unexpected type.");
                                // Relaxed: Continue checking other children
                                continue;
                            }
                        }
                        Err(e) => { // Error reading a specific directory entry
                            warn!(path = %container_candidate_path.display(), error = %e, "Error iterating directory entry.");
                            // Relaxed: Continue checking other children, though this might indicate a broader issue.
                            continue;
                        }
                    }
                }
            }
            Err(e) => { // Error reading the parent directory itself
                warn!(path = %container_candidate_path.display(), error = %e, "Could not read directory to check for bare repo container status.");
                return false; // Cannot determine, so assume not a container
            }
        }

        let is_container = worktree_children_count > 0;
        if is_container {
            debug!(path = %container_candidate_path.display(), worktree_count = worktree_children_count, "Path IS a bare repo worktree container (relaxed check).");
        } else {
            debug!(path = %container_candidate_path.display(), worktree_count = worktree_children_count, "Path is NOT a bare repo worktree container (relaxed check): no qualifying worktree children found.");
        }
        is_container
    }


    fn process_path_candidate(
        &self,
        original_path: PathBuf, // Path as found by WalkDir or from additional_paths
        is_explicitly_added: bool, // New flag
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

        let is_hidden_by_name = basename_of_resolved_path.starts_with('.') &&
                                basename_of_resolved_path.len() > 1 &&
                                basename_of_resolved_path != ".git";

        if is_hidden_by_name && !is_explicitly_added {
            debug!(name = %basename_of_resolved_path, path = %resolved_path.display(), "Skipping hidden directory (not explicitly added)");
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
                        let mut add_repo_entry_as_git_repository = true;

                        if repo.is_bare() {
                            if self.is_bare_repo_worktree_exclusive_container(&resolved_path, &repo) {
                                debug!(path = %resolved_path.display(), "Identified as a bare repo worktree exclusive container. Skipping direct entry for the container itself, but its worktrees will be listed.");
                                add_repo_entry_as_git_repository = false;
                                // Do NOT add to processed_resolved_paths here for the container itself.
                            }
                        }

                        if add_repo_entry_as_git_repository {
                            let repo_type_str = if repo.is_bare() { "bare Git repository" } else { "standard Git repository" };
                            debug!(path = %resolved_path.display(), name = %basename_of_resolved_path, type = repo_type_str, "Adding Git repository entry");
                            let repo_entry = DirectoryEntry {
                                path: original_path.clone(),
                                resolved_path: resolved_path.clone(),
                                display_name: basename_of_resolved_path.clone(),
                                entry_type: DirectoryType::GitRepository,
                                parent_path: None,
                            };
                            entries.push(repo_entry);
                            processed_resolved_paths.insert(resolved_path.clone());
                        }

                        // Always try to list linked worktrees for any repository (bare or not, container or not)
                        // that is not itself a worktree.
                        match list_linked_worktrees(&resolved_path) {
                            Ok(linked_worktrees) => {
                                debug!(repo_path = %resolved_path.display(), count = linked_worktrees.len(), "Found linked worktrees");
                                
                                // Use the resolved_path of the directory we are currently processing (the container)
                                // as the reference for the parent part of the worktree display name.
                                let main_repo_ref_path_for_display = resolved_path.clone();

                                for worktree_info in linked_worktrees {
                                    let wt_path_from_git = worktree_info.path;
                                    match fs::canonicalize(&wt_path_from_git) {
                                        Ok(canonical_wt_path) => {
                                            if !canonical_wt_path.is_dir() {
                                                warn!(wt_path = %wt_path_from_git.display(), resolved_wt_path = %canonical_wt_path.display(), "Linked worktree path is not a directory, skipping");
                                                continue;
                                            }
                                             self.add_worktree_entry(
                                                wt_path_from_git.clone(),
                                                canonical_wt_path,
                                                &main_repo_ref_path_for_display, // Use the determined reference path
                                                Some(worktree_info.name),
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
    /// This function is for paths that are NOT git repositories themselves.
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
                                                        Ok(main_repo_path_val) => {
                                                            // Ensure the path is canonical before comparison or storage.
                                                            let current_main_repo_path = match fs::canonicalize(&main_repo_path_val) {
                                                                Ok(p) => p,
                                                                Err(e) => {
                                                                    warn!(path = %main_repo_path_val.display(), error = %e, "Failed to canonicalize main repo path for worktree child {}, using as-is.", canonical_child_path.display());
                                                                    main_repo_path_val // Fallback to the path as returned by the handler
                                                                }
                                                            };

                                                            if first_main_repo_path.is_none() {
                                                                first_main_repo_path = Some(current_main_repo_path.clone());
                                                                debug!(child_worktree = %canonical_child_path.display(), main_repo = %current_main_repo_path.display(), "First worktree found, setting common main repo path.");
                                                            } else if first_main_repo_path.as_ref() != Some(&current_main_repo_path) {
                                                                debug!(child_worktree = %canonical_child_path.display(), main_repo = %current_main_repo_path.display(), expected_main_repo = ?first_main_repo_path, "Worktree belongs to a different main repo, parent not a container.");
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
                self.process_path_candidate(path_from_walkdir, false, &mut entries, &mut processed_resolved_paths); // Set is_explicitly_added to false
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
            self.process_path_candidate(original_path, true, &mut entries, &mut processed_resolved_paths); // Set is_explicitly_added to true
        }

        info!(count = entries.len(), "Directory scan complete");
        debug!(final_entries = ?entries, "Final list of directory entries");
        entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use git2::{Repository, WorktreeAddOptions, Signature}; // Added Signature
    use regex::Regex;
    use std::fs::{self, File};
    // Removed: use std::io::Write;
    use tempfile::tempdir;

    // Helper to initialize a standard git repo
    fn init_repo(path: &Path) -> Repository {
        Repository::init(path).expect("Failed to init repo")
    }

    // Helper to initialize a bare git repo
    fn init_bare_repo(path: &Path) -> Repository {
        Repository::init_bare(path).expect("Failed to init bare repo")
    }

    // Helper to add a worktree to a bare repository
    fn add_worktree_to_bare(
        bare_repo: &Repository,
        worktree_name: &str,
        worktree_path: &Path,
    ) -> Repository {
        // Create an initial commit if the repo is empty, which is necessary for worktree creation.
        if bare_repo.is_empty().unwrap_or(true) {
            let mut index = bare_repo.index().expect("Failed to get index for bare repo");
            let tree_id = index.write_tree().expect("Failed to write empty tree");
            let tree = bare_repo.find_tree(tree_id).expect("Failed to find tree");
            let sig = Signature::now("Test User", "test@example.com").expect("Failed to create signature");
            bare_repo.commit(
                Some("HEAD"),      // Update HEAD
                &sig,              // Author
                &sig,              // Committer
                "Initial commit",  // Commit message
                &tree,             // Tree
                &[],               // No parent commits
            ).expect("Failed to create initial commit in bare repo");
        }

        // For a bare repo, worktrees are added relative to its path, but the actual worktree dir can be elsewhere.
        // We need to ensure the worktree_path exists.
        fs::create_dir_all(worktree_path.parent().unwrap()).expect("Failed to create parent for worktree path");
        let mut opts = WorktreeAddOptions::new();
        bare_repo
            .worktree(worktree_name, worktree_path, Some(&mut opts))
            .expect("Failed to add worktree");
        Repository::open(worktree_path).expect("Failed to open added worktree")
    }
    
    // Helper to create a default config for tests
    fn default_test_config() -> Config {
        Config {
            search_paths: vec![],
            additional_paths: vec![],
            exclude_patterns: vec![],
            debug_mode: false, // or true for more test output
            direct_selection: None,
            // Initialize other fields if they become non-optional or affect tests
        }
    }

    // Test for the new is_bare_repo_worktree_exclusive_container
    #[test]
    fn test_is_bare_repo_worktree_exclusive_container_true() {
        let base_dir = tempdir().unwrap();
        let container_path = base_dir.path().join("project_container");
        fs::create_dir(&container_path).unwrap();

        // Create the .git file pointing to the bare repo
        let bare_repo_actual_path = container_path.join("actual_bare.git");
        fs::create_dir(&bare_repo_actual_path).unwrap(); // Create dir for bare repo
        let _bare_repo_git_object = init_bare_repo(&bare_repo_actual_path); // Initialize it

        // Create .git file in container_path
        fs::write(container_path.join(".git"), format!("gitdir: {}", bare_repo_actual_path.file_name().unwrap().to_str().unwrap())).unwrap();
        
        // Open the container as a repo (it should resolve to the bare repo)
        let container_repo = Repository::open(&container_path).expect("Failed to open container as repo");
        assert!(container_repo.is_bare(), "Container repo should be bare");

        // Add worktrees as children of container_path
        let wt1_path = container_path.join("worktree1");
        add_worktree_to_bare(&container_repo, "worktree1", &wt1_path);
        let wt2_path = container_path.join("worktree2");
        add_worktree_to_bare(&container_repo, "worktree2", &wt2_path);
        
        let config = default_test_config();
        let scanner = DirectoryScanner::new(&config);
        
        assert!(scanner.is_bare_repo_worktree_exclusive_container(&container_path, &container_repo), "Should be an exclusive bare repo container");
    }

    #[test]
    fn test_is_bare_repo_worktree_container_true_despite_extra_file_relaxed() {
        let base_dir = tempdir().unwrap();
        let container_path = base_dir.path().join("project_container_extra_file");
        fs::create_dir(&container_path).unwrap();

        let bare_repo_actual_path = container_path.join("actual_bare.git");
        fs::create_dir(&bare_repo_actual_path).unwrap();
        let _ = init_bare_repo(&bare_repo_actual_path);
        fs::write(container_path.join(".git"), format!("gitdir: {}", bare_repo_actual_path.file_name().unwrap().to_str().unwrap())).unwrap();
        
        let container_repo = Repository::open(&container_path).unwrap();

        let wt1_path = container_path.join("worktree1");
        add_worktree_to_bare(&container_repo, "worktree1", &wt1_path);
        
        File::create(container_path.join("extra_file.txt")).unwrap(); // Add an extra file

        let config = default_test_config();
        let scanner = DirectoryScanner::new(&config);
        assert!(scanner.is_bare_repo_worktree_exclusive_container(&container_path, &container_repo), "Should be a container despite extra file (relaxed check)");
    }

    #[test]
    fn test_is_bare_repo_worktree_container_true_despite_unrelated_dir_relaxed() {
        let base_dir = tempdir().unwrap();
        let container_path = base_dir.path().join("project_container_extra_dir");
        fs::create_dir(&container_path).unwrap();

        let bare_repo_actual_path = container_path.join("actual_bare.git");
        fs::create_dir(&bare_repo_actual_path).unwrap();
        let _ = init_bare_repo(&bare_repo_actual_path);
        fs::write(container_path.join(".git"), format!("gitdir: {}", bare_repo_actual_path.file_name().unwrap().to_str().unwrap())).unwrap();
        
        let container_repo = Repository::open(&container_path).unwrap();

        let wt1_path = container_path.join("worktree1");
        add_worktree_to_bare(&container_repo, "worktree1", &wt1_path);
        
        fs::create_dir(container_path.join("unrelated_dir")).unwrap(); // Add an unrelated directory

        let config = default_test_config();
        let scanner = DirectoryScanner::new(&config);
        assert!(scanner.is_bare_repo_worktree_exclusive_container(&container_path, &container_repo), "Should be a container despite unrelated dir (relaxed check)");
    }
    
    #[test]
    fn test_is_bare_repo_worktree_exclusive_container_false_no_worktrees() {
        let base_dir = tempdir().unwrap();
        let container_path = base_dir.path().join("project_container_no_wt");
        fs::create_dir(&container_path).unwrap();

        let bare_repo_actual_path = container_path.join("actual_bare.git");
        fs::create_dir(&bare_repo_actual_path).unwrap();
        let _ = init_bare_repo(&bare_repo_actual_path);
        fs::write(container_path.join(".git"), format!("gitdir: {}", bare_repo_actual_path.file_name().unwrap().to_str().unwrap())).unwrap();
        
        let container_repo = Repository::open(&container_path).unwrap();
        // No worktrees added

        let config = default_test_config();
        let scanner = DirectoryScanner::new(&config);
        assert!(!scanner.is_bare_repo_worktree_exclusive_container(&container_path, &container_repo), "Should not be an exclusive container as there are no worktrees");
    }

    #[test]
    fn test_scan_skips_bare_repo_container_lists_its_worktrees() {
        let base_dir = tempdir().unwrap();
        
        // Setup the container structure
        let container_path = base_dir.path().join("my_bare_container");
        fs::create_dir(&container_path).unwrap();
        let bare_repo_dir_name = "internal_bare.git";
        let bare_repo_actual_path = container_path.join(bare_repo_dir_name);
        fs::create_dir(&bare_repo_actual_path).unwrap();
        let _ = init_bare_repo(&bare_repo_actual_path); // Initialize the bare repo
        fs::write(container_path.join(".git"), format!("gitdir: {}", bare_repo_dir_name)).unwrap(); // Link .git file

        let container_repo_obj = Repository::open(&container_path).expect("Failed to open container path as repo for test setup");

        let wt1_path = container_path.join("feature_a");
        add_worktree_to_bare(&container_repo_obj, "feature_a", &wt1_path);
        let wt2_path = container_path.join("bugfix_b");
        add_worktree_to_bare(&container_repo_obj, "bugfix_b", &wt2_path);

        // Add a plain project for control
        let plain_project_path = base_dir.path().join("plain_old_project");
        fs::create_dir(&plain_project_path).unwrap();

        let mut config = default_test_config();
        config.search_paths = vec![base_dir.path().to_path_buf()]; // Scan children of base_dir

        let scanner = DirectoryScanner::new(&config);
        let entries = scanner.scan();

        let canonical_container_path = fs::canonicalize(&container_path).unwrap();
        let canonical_wt1_path = fs::canonicalize(&wt1_path).unwrap();
        let canonical_wt2_path = fs::canonicalize(&wt2_path).unwrap();
        let canonical_plain_project_path = fs::canonicalize(&plain_project_path).unwrap();

        let container_name = container_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();

        // The container itself should NOT be an entry
        assert!(!entries.iter().any(|e| e.resolved_path == canonical_container_path), "Bare repo container itself should be skipped. Entries: {:?}", entries);
        
        // Its worktrees SHOULD be entries
        let wt1_entry = entries.iter().find(|e| e.resolved_path == canonical_wt1_path);
        assert!(wt1_entry.is_some(), "Worktree 1 should be listed. Entries: {:?}", entries);
        assert!(matches!(wt1_entry.unwrap().entry_type, DirectoryType::GitWorktree { .. }), "Worktree 1 should be of type GitWorktree");
        assert_eq!(wt1_entry.unwrap().display_name, format!("[{}] feature_a", container_name));


        let wt2_entry = entries.iter().find(|e| e.resolved_path == canonical_wt2_path);
        assert!(wt2_entry.is_some(), "Worktree 2 should be listed. Entries: {:?}", entries);
        assert!(matches!(wt2_entry.unwrap().entry_type, DirectoryType::GitWorktree { .. }), "Worktree 2 should be of type GitWorktree");
        assert_eq!(wt2_entry.unwrap().display_name, format!("[{}] bugfix_b", container_name));


        // The plain project should be an entry
        assert!(entries.iter().any(|e| e.resolved_path == canonical_plain_project_path && e.entry_type == DirectoryType::Plain), "Plain project should be listed. Entries: {:?}", entries);

        // Total entries: wt1, wt2, plain_project = 3
        assert_eq!(entries.len(), 3, "Expected 3 entries (2 worktrees, 1 plain project). Entries: {:?}", entries);
    }


    #[test]
    fn test_check_if_worktree_container_valid_two_worktrees() {
        let main_repo_dir = tempdir().unwrap();
        let _main_repo = init_bare_repo(main_repo_dir.path());

        let container_dir = tempdir().unwrap();
        let wt1_path = container_dir.path().join("wt1");
        let wt2_path = container_dir.path().join("wt2");
        add_worktree_to_bare(&_main_repo, "wt1", &wt1_path);
        add_worktree_to_bare(&_main_repo, "wt2", &wt2_path);

        let config = default_test_config();
        let scanner = DirectoryScanner::new(&config);
        assert!(scanner.check_if_worktree_container(container_dir.path()));
    }

    #[test]
    fn test_check_if_worktree_container_one_worktree() {
        let main_repo_dir = tempdir().unwrap();
        let _main_repo = init_bare_repo(main_repo_dir.path());

        let container_dir = tempdir().unwrap();
        let wt1_path = container_dir.path().join("wt1");
        add_worktree_to_bare(&_main_repo, "wt1", &wt1_path);

        let config = default_test_config();
        let scanner = DirectoryScanner::new(&config);
        assert!(scanner.check_if_worktree_container(container_dir.path()));
    }

    #[test]
    fn test_check_if_worktree_container_empty_dir() {
        let container_dir = tempdir().unwrap();
        let config = default_test_config();
        let scanner = DirectoryScanner::new(&config);
        assert!(!scanner.check_if_worktree_container(container_dir.path()));
    }

    #[test]
    fn test_check_if_worktree_container_with_file() {
        let main_repo_dir = tempdir().unwrap();
        let _main_repo = init_bare_repo(main_repo_dir.path());

        let container_dir = tempdir().unwrap();
        let wt1_path = container_dir.path().join("wt1");
        add_worktree_to_bare(&_main_repo, "wt1", &wt1_path);
        File::create(container_dir.path().join("some_file.txt")).unwrap();

        let config = default_test_config();
        let scanner = DirectoryScanner::new(&config);
        assert!(!scanner.check_if_worktree_container(container_dir.path()));
    }

    #[test]
    fn test_check_if_worktree_container_with_plain_dir() {
        let main_repo_dir = tempdir().unwrap();
        let _main_repo = init_bare_repo(main_repo_dir.path());

        let container_dir = tempdir().unwrap();
        let wt1_path = container_dir.path().join("wt1");
        add_worktree_to_bare(&_main_repo, "wt1", &wt1_path);
        fs::create_dir(container_dir.path().join("plain_dir")).unwrap();

        let config = default_test_config();
        let scanner = DirectoryScanner::new(&config);
        assert!(!scanner.check_if_worktree_container(container_dir.path()));
    }

    #[test]
    fn test_check_if_worktree_container_with_non_worktree_repo() {
        let main_repo_dir = tempdir().unwrap();
        let _main_repo = init_bare_repo(main_repo_dir.path());

        let container_dir = tempdir().unwrap();
        let wt1_path = container_dir.path().join("wt1");
        add_worktree_to_bare(&_main_repo, "wt1", &wt1_path);
        init_repo(&container_dir.path().join("other_repo")); // Standard repo, not a worktree

        let config = default_test_config();
        let scanner = DirectoryScanner::new(&config);
        assert!(!scanner.check_if_worktree_container(container_dir.path()));
    }

    #[test]
    fn test_check_if_worktree_container_different_main_repos() {
        let main_repo_a_dir = tempdir().unwrap();
        let _main_repo_a = init_bare_repo(main_repo_a_dir.path());
        let main_repo_b_dir = tempdir().unwrap();
        let _main_repo_b = init_bare_repo(main_repo_b_dir.path());

        let container_dir = tempdir().unwrap();
        let wt_a_path = container_dir.path().join("wt_a");
        let wt_b_path = container_dir.path().join("wt_b");
        add_worktree_to_bare(&_main_repo_a, "wt_a", &wt_a_path);
        add_worktree_to_bare(&_main_repo_b, "wt_b", &wt_b_path);

        let config = default_test_config();
        let scanner = DirectoryScanner::new(&config);
        assert!(!scanner.check_if_worktree_container(container_dir.path()));
    }

    #[test]
    #[cfg(unix)] // Symlinks are tricky on Windows and might require admin rights
    fn test_check_if_worktree_container_with_symlink_to_worktree() {
        let main_repo_temp_dir = tempdir().unwrap(); // Temp dir for the bare repo
        let _main_repo = init_bare_repo(main_repo_temp_dir.path());

        let actual_wt_parent_dir = tempdir().unwrap(); // Parent for actual worktree's physical location
        let actual_wt_physical_path = actual_wt_parent_dir.path().join("actual_wt1_loc");
        // actual_wt_physical_path does not exist yet, add_worktree_to_bare will create it.
        add_worktree_to_bare(&_main_repo, "actual_wt1", &actual_wt_physical_path);
        
        let container_dir = tempdir().unwrap(); // This is the dir we are testing
        let symlink_path = container_dir.path().join("sym_wt1");
        std::os::unix::fs::symlink(&actual_wt_physical_path, symlink_path).unwrap();

        let config = default_test_config();
        let scanner = DirectoryScanner::new(&config);
        assert!(scanner.check_if_worktree_container(container_dir.path()));
    }

    #[test]
    #[cfg(unix)]
    fn test_check_if_worktree_container_with_symlink_to_file() {
        let main_repo_temp_dir = tempdir().unwrap(); // Temp dir for the bare repo
        let _main_repo = init_bare_repo(main_repo_temp_dir.path());

        let container_dir = tempdir().unwrap(); // Dir we are testing
        let wt1_path = container_dir.path().join("wt1");
        add_worktree_to_bare(&_main_repo, "wt1", &wt1_path);

        let file_target_temp_dir = tempdir().unwrap(); // TempDir for the target file, ensure it lives
        let file_path = file_target_temp_dir.path().join("target_file.txt");
        File::create(&file_path).unwrap(); // Now file_path's parent exists
        let symlink_path = container_dir.path().join("sym_to_file");
        std::os::unix::fs::symlink(&file_path, symlink_path).unwrap();
        
        let config = default_test_config();
        let scanner = DirectoryScanner::new(&config);
        assert!(!scanner.check_if_worktree_container(container_dir.path()));
    }

    #[test]
    fn test_scan_excludes_worktree_container() { // This tests the original check_if_worktree_container
        let base_dir = tempdir().unwrap();
        let main_repo_dir = base_dir.path().join("main_bare_repo_for_other_container"); // Different main repo
        fs::create_dir(&main_repo_dir).unwrap();
        let _main_repo = init_bare_repo(&main_repo_dir);

        // This container is NOT a repo itself, its children are worktrees of _main_repo
        let non_repo_container_dir_path = base_dir.path().join("non_repo_worktree_holder");
        fs::create_dir(&non_repo_container_dir_path).unwrap();
        
        let wt1_path = non_repo_container_dir_path.join("wt1_in_non_repo_container");
        add_worktree_to_bare(&_main_repo, "wt1_in_non_repo_container", &wt1_path);
        let wt2_path = non_repo_container_dir_path.join("wt2_in_non_repo_container");
        add_worktree_to_bare(&_main_repo, "wt2_in_non_repo_container", &wt2_path);

        let plain_dir_path = base_dir.path().join("plain_project_for_container_test");
        fs::create_dir(&plain_dir_path).unwrap();

        let mut config = default_test_config();
        config.search_paths = vec![base_dir.path().to_path_buf()];

        let scanner = DirectoryScanner::new(&config);
        let entries = scanner.scan();
        
        let canonical_main_repo_dir = fs::canonicalize(&main_repo_dir).unwrap();
        // wt1 and wt2 are children of non_repo_container_dir_path, which is skipped.
        // These worktrees are listed because main_repo_dir lists them.
        let canonical_wt1_path = fs::canonicalize(&wt1_path).unwrap(); 
        let canonical_wt2_path = fs::canonicalize(&wt2_path).unwrap();
        let canonical_plain_dir_path = fs::canonicalize(&plain_dir_path).unwrap();
        let canonical_container_dir_path = fs::canonicalize(&non_repo_container_dir_path).unwrap();

        assert!(entries.iter().any(|e| e.resolved_path == canonical_main_repo_dir));
        assert!(entries.iter().any(|e| e.resolved_path == canonical_wt1_path)); 
        assert!(entries.iter().any(|e| e.resolved_path == canonical_wt2_path)); 
        assert!(entries.iter().any(|e| e.resolved_path == canonical_plain_dir_path));
        assert!(!entries.iter().any(|e| e.resolved_path == canonical_container_dir_path), "Non-repo worktree container should be excluded. Entries: {:?}", entries);
        
        // main_repo_dir, its 2 worktrees, plain_project = 4 entries
        assert_eq!(entries.len(), 4, "Expected 4 entries. Entries: {:?}", entries);
    }

    #[test]
    fn test_scan_finds_various_types() {
        let base_dir = tempdir().unwrap();

        let plain_project_path = base_dir.path().join("my_plain_project");
        fs::create_dir(&plain_project_path).unwrap();

        let git_project_path = base_dir.path().join("my_git_project");
        init_repo(&git_project_path); 

        let main_bare_repo_path = base_dir.path().join("central_bare.git");
        let main_bare_repo = init_bare_repo(&main_bare_repo_path);
        
        let worktree1_path = base_dir.path().join("worktree_one"); // worktree of central_bare.git
        add_worktree_to_bare(&main_bare_repo, "wt_one", &worktree1_path);

        let mut config = default_test_config();
        config.search_paths = vec![base_dir.path().to_path_buf()];
        let scanner = DirectoryScanner::new(&config);
        let entries = scanner.scan();

        // Expected: plain_project, git_project, central_bare.git, worktree_one
        assert_eq!(entries.len(), 4, "Should find plain, git repo, bare repo, and its worktree. Entries: {:?}", entries);

        let canonical_plain_project_path = fs::canonicalize(&plain_project_path).unwrap();
        let canonical_git_project_path = fs::canonicalize(&git_project_path).unwrap();
        let canonical_main_bare_repo_path = fs::canonicalize(&main_bare_repo_path).unwrap();
        let canonical_worktree1_path = fs::canonicalize(&worktree1_path).unwrap();

        assert!(entries.iter().any(|e| e.resolved_path == canonical_plain_project_path && e.entry_type == DirectoryType::Plain));
        assert!(entries.iter().any(|e| e.resolved_path == canonical_git_project_path && e.entry_type == DirectoryType::GitRepository));
        assert!(entries.iter().any(|e| e.resolved_path == canonical_main_bare_repo_path && e.entry_type == DirectoryType::GitRepository)); // The bare repo itself is an entry
        let wt1_entry = entries.iter().find(|e| e.resolved_path == canonical_worktree1_path);
        assert!(wt1_entry.is_some());
        assert!(matches!(wt1_entry.unwrap().entry_type, DirectoryType::GitWorktree { .. }));
        assert_eq!(wt1_entry.unwrap().display_name, "[central_bare.git] worktree_one");
    }

    #[test]
    fn test_scan_with_tilde_expansion_and_additional_paths() {
        // This test is a bit conceptual for tilde as it depends on `dirs::home_dir()`
        // We'll simulate a structure that would be found if tilde expansion worked.
        let home_sim_dir = tempdir().unwrap();
        let dev_dir_in_home = home_sim_dir.path().join("Development");
        fs::create_dir_all(&dev_dir_in_home).unwrap();
        let project_in_dev_path = dev_dir_in_home.join("my_dev_project");
        fs::create_dir(&project_in_dev_path).unwrap();

        let other_loc_dir = tempdir().unwrap();
        let additional_project_path = other_loc_dir.path().join("additional_proj");
        fs::create_dir(&additional_project_path).unwrap();
        
        // Mock `expand_tilde` for this test by using paths that don't need it,
        // or ensure the test environment has a home dir.
        // For simplicity, we'll use absolute paths in config for this test,
        // assuming `expand_tilde` is tested elsewhere or works.
        // Or, we can test `expand_tilde` separately.
        // Here, we'll construct search_paths that mimic post-tilde-expansion.
        
        let mut config = default_test_config();
        // If we could mock dirs::home_dir(), we'd use "~/Development"
        // Instead, use the actual path for testing the rest of scan logic
        config.search_paths = vec![dev_dir_in_home.clone()]; 
        config.additional_paths = vec![additional_project_path.clone()];

        let scanner = DirectoryScanner::new(&config);
        let entries = scanner.scan();
        
        assert_eq!(entries.len(), 2, "Entries: {:?}", entries);

        let canonical_project_in_dev_path = fs::canonicalize(&project_in_dev_path).unwrap();
        let canonical_additional_project_path = fs::canonicalize(&additional_project_path).unwrap();
        assert!(entries.iter().any(|e| e.resolved_path == canonical_project_in_dev_path));
        assert!(entries.iter().any(|e| e.resolved_path == canonical_additional_project_path));
    }

    #[test]
    fn test_scan_exclusion_patterns() {
        let base_dir = tempdir().unwrap();
        let project_a_path = base_dir.path().join("project_a");
        fs::create_dir(&project_a_path).unwrap();
        let project_b_path = base_dir.path().join("project_b_exclude");
        fs::create_dir(&project_b_path).unwrap();

        let mut config = default_test_config();
        config.search_paths = vec![base_dir.path().to_path_buf()];
        config.exclude_patterns = vec![Regex::new("_exclude$").unwrap()];

        let scanner = DirectoryScanner::new(&config);
        let entries = scanner.scan();

        assert_eq!(entries.len(), 1);

        let canonical_project_a_path = fs::canonicalize(&project_a_path).unwrap();
        let canonical_project_b_path = fs::canonicalize(&project_b_path).unwrap();
        assert!(entries.iter().any(|e| e.resolved_path == canonical_project_a_path));
        assert!(!entries.iter().any(|e| e.resolved_path == canonical_project_b_path));
    }

     #[test]
    fn test_scan_hidden_directory_exclusion_in_walkdir() {
        let base_dir = tempdir().unwrap();
        let visible_project_path = base_dir.path().join("visible_project");
        fs::create_dir(&visible_project_path).unwrap();
        let hidden_project_path = base_dir.path().join(".hidden_project");
        fs::create_dir(&hidden_project_path).unwrap();

        let mut config = default_test_config();
        config.search_paths = vec![base_dir.path().to_path_buf()]; // Scan base_dir children

        let scanner = DirectoryScanner::new(&config);
        let entries = scanner.scan();
        
        assert_eq!(entries.len(), 1, "Only visible_project should be found. Entries: {:?}", entries);

        let canonical_visible_project_path = fs::canonicalize(&visible_project_path).unwrap();
        let canonical_hidden_project_path = fs::canonicalize(&hidden_project_path).unwrap();
        assert!(entries.iter().any(|e| e.resolved_path == canonical_visible_project_path));
        assert!(!entries.iter().any(|e| e.resolved_path == canonical_hidden_project_path));
    }

    #[test]
    fn test_scan_includes_explicitly_added_hidden_directory() {
        // e.g. if "~/.config" is an additional_path
        let base_dir = tempdir().unwrap();
        let hidden_config_path = base_dir.path().join(".myconfig");
        fs::create_dir(&hidden_config_path).unwrap();

        let mut config = default_test_config();
        config.additional_paths = vec![hidden_config_path.clone()]; // Explicitly add .myconfig

        let scanner = DirectoryScanner::new(&config);
        let entries = scanner.scan();
        
        assert_eq!(entries.len(), 1, "Explicitly added hidden dir should be found. Entries: {:?}", entries);

        let canonical_hidden_config_path = fs::canonicalize(&hidden_config_path).unwrap();
        assert!(entries.iter().any(|e| e.resolved_path == canonical_hidden_config_path));
    }
}
