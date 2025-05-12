//! Scans directories based on configuration to find potential projects (plain directories, Git repositories, Git worktrees).
//!
//! This module provides the `DirectoryScanner` struct, which takes a `Config` and performs
//! the directory traversal and identification logic. It uses libraries like `walkdir` for
//! efficient traversal and `git2` for Git repository detection. It also handles tilde expansion
//! and exclusion patterns. Parallel processing is used via Rayon to speed up the scanning
//! of multiple candidate paths.

use crate::config::Config;
use crate::container_detector;
use crate::error::Result;
use crate::git_repository_handler::{self, is_git_repository, list_linked_worktrees};
use crate::path_utils::expand_tilde;
use git2::Repository;
use rayon::prelude::*;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tracing::{Level, debug, error, info, span, warn};
use walkdir::WalkDir;

/// Represents the type of a directory entry found during scanning.
#[derive(Debug, Clone, PartialEq)]
pub enum DirectoryType {
    /// A standard directory with no special characteristics detected.
    Plain,
    /// A directory identified as the root of a Git repository (e.g., contains a `.git` directory or is bare).
    GitRepository,
    /// A directory identified as a Git worktree, linked to a main Git repository.
    GitWorktree {
        /// The canonical path to the main repository's working directory or the bare repository path.
        main_worktree_path: PathBuf,
    },
    /// A directory that primarily contains worktrees of a single main repository.
    /// This type is used internally to exclude the container directory itself from the results.
    #[allow(dead_code)] // Used conceptually for exclusion, not direct construction of entries.
    GitWorktreeContainer,
}

/// Represents a directory found during the scan, along with its metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct DirectoryEntry {
    /// The original path as discovered (e.g., from `walkdir` or config). Might contain `~` or be relative.
    pub path: PathBuf,
    /// The canonicalized, absolute path of the directory. Used for unique identification.
    pub resolved_path: PathBuf,
    /// The name used for display purposes, often the directory's basename or a formatted name for worktrees.
    pub display_name: String,
    /// The type of the directory (Plain, `GitRepository`, `GitWorktree`).
    pub entry_type: DirectoryType,
    /// For worktrees, this holds the canonical path to the main repository's working directory or bare repo path.
    pub parent_path: Option<PathBuf>,
}

/// Scans the filesystem for directories based on the provided configuration.
///
/// It identifies plain directories, Git repositories, and Git worktrees,
/// applying exclusion rules and handling tilde expansion.
pub struct DirectoryScanner<'a> {
    /// Reference to the application configuration.
    config: &'a Config,
}

impl<'a> DirectoryScanner<'a> {
    /// Creates a new `DirectoryScanner` with the given configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - A reference to the application's `Config`.
    pub fn new(config: &'a Config) -> Self {
        Self { config }
    }

    /// Creates a `DirectoryEntry` for a Git worktree.
    ///
    /// This helper function constructs the display name and sets the appropriate
    /// `DirectoryType` and `parent_path` for a worktree entry.
    ///
    /// # Arguments
    ///
    /// * `original_wt_path` - The path of the worktree as originally found.
    /// * `resolved_wt_path` - The canonicalized path of the worktree.
    /// * `main_repo_resolved_path` - The canonicalized path of the main repository.
    /// * `_worktree_name_opt` - Optional name of the worktree (currently unused in display name).
    ///
    /// # Returns
    ///
    /// A `DirectoryEntry` representing the Git worktree.
    fn add_worktree_entry(
        original_wt_path: PathBuf, // The path as found by WalkDir or from git config
        resolved_wt_path: PathBuf, // The canonicalized path of the worktree
        main_repo_resolved_path: &Path,
        _worktree_name_opt: Option<String>, // Name from git worktree list, currently not used for display name per spec
    ) -> DirectoryEntry {
        // The check for processed_resolved_paths is now done in process_path_candidate before calling this.
        // debug!(path = %resolved_wt_path.display(), "Worktree path already processed, skipping"); // This log is removed

        let worktree_basename = resolved_wt_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let parent_basename = main_repo_resolved_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let display_name = format!("[{parent_basename}] {worktree_basename}");

        debug!(path = %resolved_wt_path.display(), main_repo = %main_repo_resolved_path.display(), name = %display_name, "Creating Git worktree entry details");
        DirectoryEntry {
            path: original_wt_path,          // Use the original path
            resolved_path: resolved_wt_path, // Pass resolved_wt_path directly
            display_name,
            entry_type: DirectoryType::GitWorktree {
                main_worktree_path: main_repo_resolved_path.to_path_buf(),
            },
            parent_path: Some(main_repo_resolved_path.to_path_buf()),
        }
    }

    /// Processes a single potential directory path found during the scan.
    ///
    /// This function performs the core logic for a single path:
    /// 1. Canonicalizes the path.
    /// 2. Checks if it's already processed (using the shared `processed_resolved_paths_mux`).
    /// 3. Applies exclusion rules (patterns, hidden directories unless explicitly added).
    /// 4. Detects the directory type (Plain, Git Repository, Git Worktree, Worktree Container).
    /// 5. For Git repositories, lists linked worktrees and adds entries for them (avoiding duplicates).
    /// 6. Skips directories identified as worktree containers.
    /// 7. Creates `DirectoryEntry` structs for valid directories/worktrees found.
    ///
    /// # Arguments
    ///
    /// * `original_path` - The path as found by `WalkDir` or from `additional_paths`.
    /// * `is_explicitly_added` - True if the path came from `config.additional_paths`.
    /// * `processed_resolved_paths_mux` - A mutex guarding a shared set of canonical paths
    ///   that have already been processed or claimed, to prevent duplicates.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `Vec<DirectoryEntry>` for the entries generated from this path
    /// (which could be the path itself and/or its linked worktrees), or an `AppError` if
    /// a critical error occurred (like mutex poisoning or initial canonicalization failure).
    /// Returns `Ok(Vec::new())` if the path is skipped due to filters, duplication, or being a container.
    #[allow(clippy::too_many_lines)]
    fn process_path_candidate(
        &self,
        original_path: PathBuf,
        is_explicitly_added: bool,
        processed_resolved_paths_mux: &Mutex<HashSet<PathBuf>>,
    ) -> Result<Vec<DirectoryEntry>> {
        let candidate_span =
            span!(Level::DEBUG, "process_path_candidate", path = %original_path.display());
        let _enter = candidate_span.enter();

        let mut current_entries = Vec::new(); // Collect entries generated by this path

        let resolved_path = match fs::canonicalize(&original_path) {
            Ok(p) => p,
            Err(e) => {
                warn!(original_path = %original_path.display(), error = %e, "Could not canonicalize path, skipping this path");
                return Err(e.into()); // Propagate error for this path
            }
        };

        if !resolved_path.is_dir() {
            debug!(original_path = %original_path.display(), resolved_path = %resolved_path.display(), "Skipping as resolved path is not a directory");
            return Ok(Vec::new()); // Return empty vec for skip
        }

        // Explicitly skip .git directories found during scanning.
        if resolved_path.file_name().is_some_and(|name| name == ".git") {
            debug!(path = %resolved_path.display(), "Skipping .git directory");
            return Ok(Vec::new()); // Return empty vec for skip
        }

        debug!(original = %original_path.display(), resolved = %resolved_path.display(), "Path resolved");

        // Critical section for checking and inserting into processed_resolved_paths
        {
            let mut processed_paths_guard = processed_resolved_paths_mux.lock().map_err(|e| {
                error!("Mutex poisoned while accessing processed_resolved_paths: {e}");
                crate::error::AppError::MutexError(format!(
                    "Mutex poisoned while accessing processed_resolved_paths: {e}"
                ))
            })?;

            if processed_paths_guard.contains(&resolved_path) {
                debug!(path = %resolved_path.display(), "Skipping duplicate resolved path (checked in parallel context)");
                return Ok(Vec::new()); // Return empty vec for skip
            }
            // If we are going to process it, add it to the set.
            processed_paths_guard.insert(resolved_path.clone());
        } // Lock released

        for pattern in &self.config.exclude_patterns {
            if pattern.is_match(original_path.to_string_lossy().as_ref())
                || pattern.is_match(resolved_path.to_string_lossy().as_ref())
            {
                debug!(path = %resolved_path.display(), pattern = %pattern, "Skipping excluded path");
                return Ok(Vec::new()); // Return empty vec for skip
            }
        }

        let basename_of_resolved_path = resolved_path.file_name().map_or_else(
            || {
                original_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            },
            |os_str| os_str.to_string_lossy().into_owned(),
        );

        let is_hidden_by_name = basename_of_resolved_path.starts_with('.')
            && basename_of_resolved_path.len() > 1
            && basename_of_resolved_path != ".git";

        if is_hidden_by_name && !is_explicitly_added {
            debug!(name = %basename_of_resolved_path, path = %resolved_path.display(), "Skipping hidden directory (not explicitly added)");
            return Ok(Vec::new()); // Return empty vec for skip
        }

        if is_git_repository(&resolved_path) {
            match Repository::open(&resolved_path) {
                Ok(repo) => {
                    if repo.is_worktree() {
                        match git_repository_handler::get_main_repository_path(&resolved_path) {
                            Ok(main_repo_path) => {
                                current_entries.push(Self::add_worktree_entry(
                                    original_path.clone(),
                                    resolved_path.clone(),
                                    &main_repo_path,
                                    None,
                                ));
                            }
                            Err(e) => {
                                warn!(path = %resolved_path.display(), error = %e, "Failed to get main repository path for worktree, treating as plain directory");
                                current_entries.push(Self::add_plain_directory_entry(
                                    original_path,
                                    resolved_path,
                                    basename_of_resolved_path,
                                ));
                            }
                        }
                    } else {
                        let mut add_repo_entry_as_git_repository = true;

                        if repo.is_bare()
                            && container_detector::is_bare_repo_worktree_exclusive_container(
                                &resolved_path,
                                &repo,
                            )?
                        {
                            debug!(path = %resolved_path.display(), "Identified as a bare repo worktree exclusive container. Skipping direct entry for the container itself, but its worktrees will be listed.");
                            add_repo_entry_as_git_repository = false;
                        }

                        if add_repo_entry_as_git_repository {
                            let repo_type_str = if repo.is_bare() {
                                "bare Git repository"
                            } else {
                                "standard Git repository"
                            };
                            debug!(path = %resolved_path.display(), name = %basename_of_resolved_path, type = repo_type_str, "Adding Git repository entry");
                            let repo_entry = DirectoryEntry {
                                path: original_path.clone(),
                                resolved_path: resolved_path.clone(),
                                display_name: basename_of_resolved_path.clone(),
                                entry_type: DirectoryType::GitRepository,
                                parent_path: None,
                            };
                            current_entries.push(repo_entry);
                        }

                        match list_linked_worktrees(&resolved_path) {
                            Ok(linked_worktrees) => {
                                debug!(repo_path = %resolved_path.display(), count = linked_worktrees.len(), "Found linked worktrees");
                                let main_repo_ref_path_for_display = resolved_path.clone();
                                for worktree_info in linked_worktrees {
                                    let wt_path_from_git = worktree_info.path;
                                    match fs::canonicalize(&wt_path_from_git) {
                                        Ok(canonical_wt_path) => {
                                            if !canonical_wt_path.is_dir() {
                                                warn!(wt_path = %wt_path_from_git.display(), resolved_wt_path = %canonical_wt_path.display(), "Linked worktree path is not a directory, skipping");
                                                continue;
                                            }

                                            // Check and claim the canonical_wt_path in the shared set.
                                            let mut processed_paths_guard = processed_resolved_paths_mux.lock().map_err(|e| {
                                                    error!("Mutex poisoned while checking linked worktree {}: {}", canonical_wt_path.display(), e);
                                                    crate::error::AppError::MutexError(format!("Mutex poisoned while checking linked worktree {}: {}", canonical_wt_path.display(), e))
                                                })?;

                                            if processed_paths_guard.contains(&canonical_wt_path) {
                                                debug!(path = %canonical_wt_path.display(), "Skipping linked worktree as its path is already processed or claimed by another scan item");
                                                // Release guard explicitly as we are continuing the loop
                                                drop(processed_paths_guard);
                                            } else {
                                                // Not processed, so claim it and add the entry.
                                                processed_paths_guard
                                                    .insert(canonical_wt_path.clone());
                                                // Release guard explicitly after insertion.
                                                drop(processed_paths_guard);

                                                current_entries.push(Self::add_worktree_entry(
                                                    wt_path_from_git.clone(),
                                                    canonical_wt_path, // This is the resolved path of the worktree
                                                    &main_repo_ref_path_for_display,
                                                    Some(worktree_info.name),
                                                ));
                                            }
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
                    if container_detector::check_if_worktree_container(&resolved_path)? {
                        debug!(path = %resolved_path.display(), "Identified as a Git worktree container (after failing to open as repo), skipping");
                        return Ok(Vec::new()); // Return empty vec for skip
                    }
                    current_entries.push(Self::add_plain_directory_entry(
                        original_path,
                        resolved_path,
                        basename_of_resolved_path,
                    ));
                }
            }
        } else {
            if container_detector::check_if_worktree_container(&resolved_path)? {
                debug!(path = %resolved_path.display(), "Identified as a Git worktree container, skipping");
                return Ok(Vec::new()); // Return empty vec for skip
            }
            current_entries.push(Self::add_plain_directory_entry(
                original_path,
                resolved_path,
                basename_of_resolved_path,
            ));
        }
        Ok(current_entries)
    }

    /// Creates a `DirectoryEntry` for a plain directory.
    ///
    /// # Arguments
    ///
    /// * `original_path` - The path as originally found.
    /// * `resolved_path` - The canonicalized path of the directory.
    /// * `display_name` - The basename of the resolved path, used for display.
    ///
    /// # Returns
    ///
    /// A `DirectoryEntry` representing the plain directory.
    fn add_plain_directory_entry(
        original_path: PathBuf,
        resolved_path: PathBuf,
        display_name: String,
    ) -> DirectoryEntry {
        debug!(name = %display_name, path = %resolved_path.display(), "Creating plain directory entry details");
        DirectoryEntry {
            path: original_path,
            resolved_path,
            display_name,
            entry_type: DirectoryType::Plain,
            parent_path: None,
        }
    }

    /// Performs the directory scan based on the configuration.
    ///
    /// This is the main entry point for the scanner. It:
    /// 1. Initializes data structures (path list, processed paths set).
    /// 2. Collects initial paths from `config.search_paths` (non-recursive children)
    ///    and `config.additional_paths`. Handles tilde expansion.
    /// 3. Uses Rayon to process the collected paths in parallel via `process_path_candidate`.
    /// 4. Consolidates the results from parallel processing.
    /// 5. Returns the final list of unique `DirectoryEntry` items.
    ///
    /// # Returns
    ///
    /// A `Vec<DirectoryEntry>` containing all valid and unique directories/worktrees found.
    pub fn scan(&self) -> Vec<DirectoryEntry> {
        let scan_span = span!(Level::INFO, "directory_scan");
        let _enter = scan_span.enter();
        info!("Starting directory scan");

        let processed_resolved_paths_mux = Mutex::new(HashSet::new());
        let mut paths_to_process: Vec<(PathBuf, bool)> = Vec::new(); // (path, is_explicitly_added)

        debug!(search_paths = ?self.config.search_paths, "Collecting paths from search_paths");
        for search_path_config_entry in &self.config.search_paths {
            let path_span = span!(Level::DEBUG, "collect_search_root", config_path = %search_path_config_entry.display());
            let _path_enter = path_span.enter();

            let Some(search_path_base) = expand_tilde(search_path_config_entry) else {
                warn!(path = %search_path_config_entry.display(), "Could not expand tilde for search path, skipping");
                continue;
            };
            debug!(expanded_path = %search_path_base.display(), "Expanded search path");

            if !search_path_base.is_dir() {
                warn!(path = %search_path_base.display(), "Search path is not a directory or is inaccessible, skipping");
                continue;
            }

            debug!(path = %search_path_base.display(), "Collecting direct children for parallel processing");
            paths_to_process.extend(
                WalkDir::new(&search_path_base)
                    .min_depth(1)
                    .max_depth(1)
                    .follow_links(true)
                    .into_iter()
                    .filter_map(|e_result| match e_result {
                        Ok(entry) => Some((entry.path().to_path_buf(), false)), // false for is_explicitly_added
                        Err(err_val) => {
                            let io_error_string = err_val
                                .io_error()
                                .map_or_else(|| "N/A".to_string(), std::string::ToString::to_string);
                            warn!(path = ?err_val.path(), error = %io_error_string, "Error walking directory child, skipping this child");
                            None
                        }
                    }),
            );
        }

        debug!(additional_paths = ?self.config.additional_paths, "Collecting additional paths");
        for additional_path_config_entry in &self.config.additional_paths {
            let path_span = span!(Level::DEBUG, "collect_additional_path", config_path = %additional_path_config_entry.display());
            let _path_enter = path_span.enter();

            let Some(original_path) = expand_tilde(additional_path_config_entry) else {
                warn!(path = %additional_path_config_entry.display(), "Could not expand tilde for additional path, skipping");
                continue;
            };
            debug!(expanded_path = %original_path.display(), "Expanded additional path");
            paths_to_process.push((original_path, true)); // true for is_explicitly_added
        }

        info!(
            count = paths_to_process.len(),
            "Collected initial paths for parallel processing"
        );

        // Parallel processing of collected paths
        let results: Vec<Result<Vec<DirectoryEntry>>> = paths_to_process
            .into_par_iter()
            .map(|(path, is_explicit)| {
                self.process_path_candidate(path, is_explicit, &processed_resolved_paths_mux)
            })
            .collect();

        // Consolidate results from parallel tasks
        let mut all_entries = Vec::new();
        for result_vec in results {
            match result_vec {
                Ok(entries_vec) => {
                    all_entries.extend(entries_vec);
                }
                Err(e) => {
                    // Log errors from individual path processing but continue with other results.
                    // These errors are specific to one path (e.g., canonicalization failure).
                    warn!(
                        "Error processing a path during scan: {}. This path was skipped.",
                        e
                    );
                }
            }
        }

        // Deduplication based on resolved_path is handled *inside* process_path_candidate
        // using the Mutex-guarded HashSet. So, `all_entries` should be unique by resolved_path
        // for top-level entries. Worktrees added as children of a repo are not subject to this
        // top-level Mutex check unless their path also happened to be a top-level scan item.

        info!(
            count = all_entries.len(),
            "Directory scan complete (after parallel processing and consolidation)"
        );
        debug!(final_entries = ?all_entries, "Final list of directory entries");
        all_entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::directory_scanner::DirectoryType;
    use git2::{Repository, Signature, WorktreeAddOptions};
    use regex::Regex;
    use std::fs::{self, File};
    use std::path::Path;
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
    fn add_worktree_to_bare(bare_repo: &Repository, worktree_name: &str, worktree_path: &Path) {
        // Create an initial commit if the repo is empty, which is necessary for worktree creation.
        if bare_repo.is_empty().unwrap_or(true) {
            let mut index = bare_repo
                .index()
                .expect("Failed to get index for bare repo");
            let tree_id = index.write_tree().expect("Failed to write empty tree");
            let tree = bare_repo.find_tree(tree_id).expect("Failed to find tree");
            let sig = Signature::now("Test User", "test@example.com")
                .expect("Failed to create signature");
            bare_repo
                .commit(
                    Some("HEAD"),     // Update HEAD
                    &sig,             // Author
                    &sig,             // Committer
                    "Initial commit", // Commit message
                    &tree,            // Tree
                    &[],              // No parent commits
                )
                .expect("Failed to create initial commit in bare repo");
        }

        let opts = WorktreeAddOptions::new();
        // opts.reference(Some(&bare_repo.head().unwrap().peel_to_commit().unwrap().id().into()));
        // The above is more robust but requires a valid HEAD. Simpler:
        // opts.reference(None); // This should checkout HEAD by default if available

        bare_repo
            .worktree(worktree_name, worktree_path, Some(&opts))
            .unwrap_or_else(|_| {
                panic!("Failed to add worktree '{worktree_name}' at path {worktree_path:?}")
            });
    }

    // New helper: Add a worktree to a standard repository
    fn add_worktree_to_standard_repo(
        repo_path: &Path,              // Path to the standard repository's working directory
        worktree_name: &str,           // Name for the new worktree (e.g., "feature-branch")
        worktree_checkout_path: &Path, // Path where the new worktree will be checked out
    ) {
        let repo = Repository::open(repo_path).expect("Failed to open repo for adding worktree");
        // Create an initial commit if the repo has no HEAD (is empty)
        if repo.head().is_err() {
            let mut index = repo.index().expect("Failed to get repo index");
            // Create an empty file to be able to create a non-empty tree
            let repo_file_path = repo_path.join("initial_file.txt");
            File::create(&repo_file_path).expect("Failed to create initial file in repo");
            index
                .add_path(Path::new("initial_file.txt"))
                .expect("Failed to add file to index");

            let id = index.write_tree().expect("Failed to write tree");
            let tree = repo.find_tree(id).expect("Failed to find tree");
            let sig =
                Signature::now("test", "test@example.com").expect("Failed to create signature");
            repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
                .expect("Failed to create initial commit");
        }
        // Ensure the worktree path parent directory exists
        if let Some(parent) = worktree_checkout_path.parent() {
            fs::create_dir_all(parent).expect("Failed to create parent directory for worktree");
        }
        repo.worktree(worktree_name, worktree_checkout_path, None) // None for default options
            .unwrap_or_else(|_| {
                panic!("Failed to add worktree {worktree_name} at {worktree_checkout_path:?}")
            });
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

    // Helper to find an entry by a suffix of its resolved_path
    // and assert its properties.
    fn assert_entry_properties(
        entries: &[DirectoryEntry],
        path_suffix: &str,
        expected_type: &str, // Use string representation for easier comparison in tests
        expected_display_name: &str,
    ) {
        let entry = entries
            .iter()
            .find(|e| e.resolved_path.ends_with(path_suffix))
            .unwrap_or_else(|| {
                panic!("Entry ending with '{path_suffix}' not found in entries: {entries:?}")
            }); // Added entries to panic msg

        match &entry.entry_type {
            DirectoryType::Plain => assert_eq!(expected_type, "Plain"),
            DirectoryType::GitRepository => assert_eq!(expected_type, "GitRepository"),
            DirectoryType::GitWorktreeContainer => {
                assert_eq!(expected_type, "GitWorktreeContainer");
            } // This type might not be directly asserted often
            DirectoryType::GitWorktree {
                main_worktree_path: _,
            } => assert_eq!(expected_type, "GitWorktree"), // Adjusted match arm
        }
        assert_eq!(entry.display_name, expected_display_name);

        // Specific checks for GitWorktree
        if expected_type == "GitWorktree" {
            if let DirectoryType::GitWorktree {
                main_worktree_path, ..
            } = &entry.entry_type
            {
                // Removed worktree_name check here as it's not stored in DirectoryEntry
                // Display name for worktree is usually "[main_repo_name] worktree_basename"
                // We can check if the expected_display_name matches the format
                assert!(expected_display_name.contains('[') && expected_display_name.contains(']'));
                // main_worktree_path should be valid.
                assert!(
                    main_worktree_path.exists(),
                    "Main worktree path {main_worktree_path:?} does not exist"
                );
            } else {
                panic!(
                    "Mismatched type: expected GitWorktree, found something else after string match."
                );
            }
        }
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
        let _bare_repo = init_bare_repo(&bare_repo_actual_path); // Initialize the bare repo
        // Simulate the .git file pointing to the bare repo dir, making container_path act like a repo
        fs::write(
            container_path.join(".git"),
            format!("gitdir: {bare_repo_dir_name}"),
        )
        .unwrap();

        // Open the container path as the repo object to add worktrees
        let container_repo_obj = Repository::open(&container_path)
            .expect("Failed to open container path as repo for test setup");

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

        // The container itself should NOT be an entry because it's detected as a bare repo exclusive container
        assert!(
            !entries
                .iter()
                .any(|e| e.resolved_path == canonical_container_path),
            "Bare repo container itself should be skipped. Entries: {:?}",
            &entries
        );

        // Its worktrees SHOULD be entries, listed via the container repo processing
        let wt1_entry = entries
            .iter()
            .find(|e| e.resolved_path == canonical_wt1_path);
        assert!(
            wt1_entry.is_some(),
            "Worktree 1 should be listed. Entries: {:?}",
            &entries
        );
        assert!(
            matches!(
                wt1_entry.unwrap().entry_type,
                DirectoryType::GitWorktree { .. }
            ),
            "Worktree 1 should be of type GitWorktree"
        );
        assert_eq!(
            wt1_entry.unwrap().display_name,
            format!("[{container_name}] feature_a")
        );

        let wt2_entry = entries
            .iter()
            .find(|e| e.resolved_path == canonical_wt2_path);
        assert!(
            wt2_entry.is_some(),
            "Worktree 2 should be listed. Entries: {:?}",
            &entries
        );
        assert!(
            matches!(
                wt2_entry.unwrap().entry_type,
                DirectoryType::GitWorktree { .. }
            ),
            "Worktree 2 should be of type GitWorktree"
        );
        assert_eq!(
            wt2_entry.unwrap().display_name,
            format!("[{container_name}] bugfix_b")
        );

        // The plain project should be an entry
        assert!(
            entries
                .iter()
                .any(|e| e.resolved_path == canonical_plain_project_path
                    && e.entry_type == DirectoryType::Plain),
            "Plain project should be listed. Entries: {:?}",
            &entries
        );

        // Total entries: wt1, wt2, plain_project = 3
        // The container itself is skipped. Its worktrees are found via list_linked_worktrees.
        // The plain project is found by WalkDir.
        assert_eq!(
            entries.len(),
            3,
            "Expected 3 entries (2 worktrees, 1 plain project). Entries: {:?}",
            &entries
        );
    }

    #[test]
    fn test_scan_excludes_worktree_container() {
        // This tests the check_if_worktree_container for non-repo directories
        let base_dir = tempdir().unwrap();
        let main_repo_dir = base_dir.path().join("main_bare_repo_for_other_container");
        fs::create_dir(&main_repo_dir).unwrap();
        let main_repo = init_bare_repo(&main_repo_dir);

        // This container is NOT a repo itself, its children are worktrees of main_repo
        let non_repo_container_dir_path = base_dir.path().join("non_repo_worktree_holder");
        fs::create_dir(&non_repo_container_dir_path).unwrap();

        let wt1_path = non_repo_container_dir_path.join("wt1_in_non_repo_container");
        add_worktree_to_bare(&main_repo, "wt1_in_non_repo_container", &wt1_path);
        let wt2_path = non_repo_container_dir_path.join("wt2_in_non_repo_container");
        add_worktree_to_bare(&main_repo, "wt2_in_non_repo_container", &wt2_path);

        let plain_dir_path = base_dir.path().join("plain_project_for_container_test");
        fs::create_dir(&plain_dir_path).unwrap();

        let mut config = default_test_config();
        config.search_paths = vec![base_dir.path().to_path_buf()]; // Scan children of base_dir

        let scanner = DirectoryScanner::new(&config);
        let entries = scanner.scan();

        let canonical_main_repo_dir = fs::canonicalize(&main_repo_dir).unwrap();
        let canonical_wt1_path = fs::canonicalize(&wt1_path).unwrap();
        let canonical_wt2_path = fs::canonicalize(&wt2_path).unwrap();
        let canonical_plain_dir_path = fs::canonicalize(&plain_dir_path).unwrap();
        let canonical_container_dir_path = fs::canonicalize(&non_repo_container_dir_path).unwrap();

        // main_repo_dir is found by WalkDir, processed, lists its worktrees (wt1, wt2)
        assert!(
            entries
                .iter()
                .any(|e| e.resolved_path == canonical_main_repo_dir),
            "Main bare repo should be listed"
        );
        // non_repo_container_dir_path is found by WalkDir, processed, identified as container, skipped
        assert!(
            !entries
                .iter()
                .any(|e| e.resolved_path == canonical_container_dir_path),
            "Non-repo worktree container should be excluded. Entries: {:?}",
            &entries
        );
        // wt1 and wt2 are listed because main_repo_dir listed them
        assert!(
            entries
                .iter()
                .any(|e| e.resolved_path == canonical_wt1_path),
            "Worktree 1 should be listed"
        );
        assert!(
            entries
                .iter()
                .any(|e| e.resolved_path == canonical_wt2_path),
            "Worktree 2 should be listed"
        );
        // plain_dir_path is found by WalkDir, processed as Plain
        assert!(
            entries
                .iter()
                .any(|e| e.resolved_path == canonical_plain_dir_path),
            "Plain project should be listed"
        );

        // Total entries: main_repo_dir, wt1, wt2, plain_dir_path = 4
        assert_eq!(
            entries.len(),
            4,
            "Expected 4 entries (main repo, 2 worktrees, 1 plain). Entries: {:?}",
            &entries
        );
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
        config.search_paths = vec![base_dir.path().to_path_buf()]; // Scan children of base_dir
        let scanner = DirectoryScanner::new(&config);
        let entries = scanner.scan();

        // Expected entries found by WalkDir:
        // - my_plain_project (Plain)
        // - my_git_project (GitRepository)
        // - central_bare.git (GitRepository, lists wt_one)
        // - worktree_one (GitWorktree, linked to central_bare.git)
        // Deduplication via Mutex ensures worktree_one is only listed once.
        assert_eq!(
            entries.len(),
            4,
            "Should find plain, git repo, bare repo, and its worktree. Entries: {:?}",
            &entries
        );

        let canonical_plain_project_path = fs::canonicalize(&plain_project_path).unwrap();
        let canonical_git_project_path = fs::canonicalize(&git_project_path).unwrap();
        let canonical_main_bare_repo_path = fs::canonicalize(&main_bare_repo_path).unwrap();
        let canonical_worktree1_path = fs::canonicalize(&worktree1_path).unwrap();

        assert!(
            entries
                .iter()
                .any(|e| e.resolved_path == canonical_plain_project_path
                    && e.entry_type == DirectoryType::Plain)
        );
        assert!(
            entries
                .iter()
                .any(|e| e.resolved_path == canonical_git_project_path
                    && e.entry_type == DirectoryType::GitRepository)
        );
        // The bare repo itself is an entry (assuming not detected as exclusive container)
        assert!(
            entries
                .iter()
                .any(|e| e.resolved_path == canonical_main_bare_repo_path
                    && e.entry_type == DirectoryType::GitRepository)
        );
        let wt1_entry = entries
            .iter()
            .find(|e| e.resolved_path == canonical_worktree1_path);
        assert!(wt1_entry.is_some());
        assert!(matches!(
            wt1_entry.unwrap().entry_type,
            DirectoryType::GitWorktree { .. }
        ));
        assert_eq!(
            wt1_entry.unwrap().display_name,
            "[central_bare.git] worktree_one"
        );
    }

    #[test]
    fn test_scan_with_tilde_expansion_and_additional_paths() {
        // This test relies on the actual home directory existing.
        // It simulates adding "~/test_dev_project" and "/tmp/other_proj" (using tempdir for the latter).
        let home_dir = dirs::home_dir().expect("Cannot run test without a home directory");
        let dev_project_in_home = home_dir.join("test_dev_project_for_scanner");
        fs::create_dir_all(&dev_project_in_home).expect("Failed to create test dir in home");

        let other_loc_dir = tempdir().unwrap();
        let additional_project_path = other_loc_dir.path().join("additional_proj");
        fs::create_dir(&additional_project_path).unwrap();

        let mut config = default_test_config();
        // Use a path starting with "~/" for search_paths
        config.search_paths = vec![PathBuf::from("~/test_dev_project_for_scanner")];
        // Use an absolute path for additional_paths
        config.additional_paths = vec![additional_project_path.clone()];

        let scanner = DirectoryScanner::new(&config);
        let entries = scanner.scan();

        // Clean up the directory created in home
        let _ = fs::remove_dir_all(&dev_project_in_home);

        // WalkDir on "~/test_dev_project_for_scanner" finds nothing inside it.
        // The additional path "additional_proj" is added directly.
        assert_eq!(entries.len(), 1, "Entries: {:?}", &entries);

        let canonical_additional_project_path = fs::canonicalize(&additional_project_path).unwrap();
        assert!(
            entries
                .iter()
                .any(|e| e.resolved_path == canonical_additional_project_path),
            "Additional project was not found"
        );
        // The search path itself ("~/test_dev_project_for_scanner") is not listed, only its contents (none).
        assert!(
            !entries
                .iter()
                .any(|e| e.resolved_path == dev_project_in_home),
            "Search path itself should not be listed"
        );
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
        assert!(
            entries
                .iter()
                .any(|e| e.resolved_path == canonical_project_a_path)
        );
        assert!(
            !entries
                .iter()
                .any(|e| e.resolved_path == canonical_project_b_path)
        );
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

        assert_eq!(
            entries.len(),
            1,
            "Only visible_project should be found. Entries: {:?}",
            &entries
        );

        let canonical_visible_project_path = fs::canonicalize(&visible_project_path).unwrap();
        let canonical_hidden_project_path = fs::canonicalize(&hidden_project_path).unwrap();
        assert!(
            entries
                .iter()
                .any(|e| e.resolved_path == canonical_visible_project_path)
        );
        assert!(
            !entries
                .iter()
                .any(|e| e.resolved_path == canonical_hidden_project_path)
        );
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

        assert_eq!(
            entries.len(),
            1,
            "Explicitly added hidden dir should be found. Entries: {:?}",
            &entries
        );

        let canonical_hidden_config_path = fs::canonicalize(&hidden_config_path).unwrap();
        assert!(
            entries
                .iter()
                .any(|e| e.resolved_path == canonical_hidden_config_path)
        );
    }

    #[test]
    fn test_scan_empty_directory() {
        let temp_dir = tempdir().unwrap();
        let mut config = default_test_config();
        config.search_paths = vec![temp_dir.path().to_path_buf()];

        let scanner = DirectoryScanner::new(&config);
        let entries = scanner.scan();

        assert!(
            entries.is_empty(),
            "Scan of empty directory should yield no entries"
        );
    }

    #[test]
    fn test_scan_single_level_directories_plain() {
        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join("dir1")).unwrap();
        fs::create_dir(temp_dir.path().join("dir2")).unwrap();

        let mut config = default_test_config();
        config.search_paths = vec![temp_dir.path().to_path_buf()];

        let scanner = DirectoryScanner::new(&config);
        let mut entries = scanner.scan();
        entries.sort_by(|a, b| a.resolved_path.cmp(&b.resolved_path));

        assert_eq!(entries.len(), 2);
        assert_entry_properties(&entries, "dir1", "Plain", "dir1");
        assert_entry_properties(&entries, "dir2", "Plain", "dir2");
    }

    #[test]
    fn test_scan_explicitly_added_path_no_recursion() {
        let temp_dir = tempdir().unwrap();
        let project_root = temp_dir.path().join("project_root");
        fs::create_dir_all(project_root.join("subdir")).unwrap();

        let mut config = default_test_config();
        config.additional_paths = vec![project_root.clone()];

        let scanner = DirectoryScanner::new(&config);
        let entries = scanner.scan();

        assert_eq!(entries.len(), 1);
        assert_entry_properties(&entries, "project_root", "Plain", "project_root");
        // "subdir" should not be listed as an entry
        assert!(!entries.iter().any(|e| e.resolved_path.ends_with("subdir")));
    }

    #[test]
    fn test_scan_search_path_one_level_recursion() {
        let temp_dir = tempdir().unwrap();
        let level1_path = temp_dir.path().join("level1");
        fs::create_dir_all(level1_path.join("level2")).unwrap();

        let mut config = default_test_config();
        config.search_paths = vec![temp_dir.path().to_path_buf()];

        let scanner = DirectoryScanner::new(&config);
        let entries = scanner.scan();

        assert_eq!(entries.len(), 1);
        assert_entry_properties(&entries, "level1", "Plain", "level1");
        // "level2" should not be listed
        assert!(!entries.iter().any(|e| e.resolved_path.ends_with("level2")));
    }

    #[test]
    fn test_scan_with_exclude_pattern_simple() {
        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join("project1")).unwrap();
        fs::create_dir(temp_dir.path().join("node_modules")).unwrap();
        fs::create_dir(temp_dir.path().join("project2")).unwrap();

        let mut config = default_test_config();
        config.search_paths = vec![temp_dir.path().to_path_buf()];
        config.exclude_patterns = vec![Regex::new("node_modules").unwrap()];

        let scanner = DirectoryScanner::new(&config);
        let mut entries = scanner.scan();
        entries.sort_by(|a, b| a.resolved_path.cmp(&b.resolved_path));

        assert_eq!(entries.len(), 2);
        assert_entry_properties(&entries, "project1", "Plain", "project1");
        assert_entry_properties(&entries, "project2", "Plain", "project2");
        assert!(
            !entries
                .iter()
                .any(|e| e.resolved_path.ends_with("node_modules"))
        );
    }

    #[test]
    fn test_scan_with_exclude_pattern_wildcard_directory() {
        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join("project_code")).unwrap();
        fs::create_dir(temp_dir.path().join("logs_dir_main")).unwrap();
        fs::create_dir(temp_dir.path().join("another_dir")).unwrap();

        let mut config = default_test_config();
        config.search_paths = vec![temp_dir.path().to_path_buf()];
        config.exclude_patterns = vec![Regex::new(r"logs_dir.*").unwrap()];

        let scanner = DirectoryScanner::new(&config);
        let mut entries = scanner.scan();
        entries.sort_by(|a, b| a.resolved_path.cmp(&b.resolved_path));

        assert_eq!(entries.len(), 2);
        assert_entry_properties(&entries, "another_dir", "Plain", "another_dir");
        assert_entry_properties(&entries, "project_code", "Plain", "project_code");
        assert!(
            !entries
                .iter()
                .any(|e| e.resolved_path.ends_with("logs_dir_main"))
        );
    }

    #[test]
    fn test_scan_with_multiple_exclude_patterns() {
        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join("src")).unwrap();
        fs::create_dir(temp_dir.path().join("target")).unwrap();
        fs::create_dir(temp_dir.path().join("docs")).unwrap();
        fs::create_dir(temp_dir.path().join("vendor")).unwrap();

        let mut config = default_test_config();
        config.search_paths = vec![temp_dir.path().to_path_buf()];
        config.exclude_patterns = vec![
            Regex::new("target$").unwrap(), // Match directory name at the end
            Regex::new("vendor$").unwrap(),
        ];

        let scanner = DirectoryScanner::new(&config);
        let mut entries = scanner.scan();
        entries.sort_by(|a, b| a.resolved_path.cmp(&b.resolved_path));

        assert_eq!(entries.len(), 2);
        assert_entry_properties(&entries, "docs", "Plain", "docs");
        assert_entry_properties(&entries, "src", "Plain", "src");
    }

    #[test]
    fn test_scan_standard_git_repository() {
        let temp_dir = tempdir().unwrap();
        let repo_path = temp_dir.path().join("my_repo");
        init_repo(&repo_path);

        let mut config = default_test_config();
        config.search_paths = vec![temp_dir.path().to_path_buf()];

        let scanner = DirectoryScanner::new(&config);
        let entries = scanner.scan();

        assert_eq!(entries.len(), 1);
        assert_entry_properties(&entries, "my_repo", "GitRepository", "my_repo");
    }

    #[test]
    fn test_scan_bare_git_repository_as_container() {
        // Test setup where a bare repo acts as a container via a .git file link
        let temp_dir = tempdir().unwrap();
        let container_path = temp_dir.path().join("bare_container");
        fs::create_dir(&container_path).unwrap();
        let bare_repo_actual_path = container_path.join("internal_bare.git");
        init_bare_repo(&bare_repo_actual_path);
        fs::write(
            container_path.join(".git"),
            format!("gitdir: {}", bare_repo_actual_path.display()),
        )
        .unwrap();

        // Add a worktree linked to the bare repo via the container path
        let worktrees_dir = temp_dir.path().join("worktrees_of_bare_container");
        fs::create_dir(&worktrees_dir).unwrap();
        let wt_a_path = worktrees_dir.join("wt_a");
        // Open the container path as the repo object to add worktrees
        let container_repo_obj = Repository::open(&container_path)
            .expect("Failed to open container path as repo for test setup");
        add_worktree_to_bare(&container_repo_obj, "wt_a", &wt_a_path);

        let mut config = default_test_config();
        config.search_paths = vec![temp_dir.path().to_path_buf()]; // Scan parent

        let scanner = DirectoryScanner::new(&config);
        let entries = scanner.scan();

        // Expected entries:
        // - bare_container (processed as GitRepository because it contains a .git file pointing to a bare repo)
        // - worktrees_of_bare_container (processed as Plain, but skipped by the worktree container check)
        // - wt_a (processed as GitWorktree, linked to bare_container, found when processing bare_container)
        // The container check for worktrees_of_bare_container should skip it.

        let bare_container_entry = entries
            .iter()
            .find(|e| e.resolved_path.ends_with("bare_container"));
        assert!(
            bare_container_entry.is_some(),
            "Bare repo container itself should be listed. Entries: {entries:?}"
        );

        let worktrees_dir_entry = entries
            .iter()
            .find(|e| e.resolved_path.ends_with("worktrees_of_bare_container"));
        assert!(
            worktrees_dir_entry.is_none(),
            "Worktree container dir should be skipped. Entries: {entries:?}"
        );

        let wt_a_entry = entries.iter().find(|e| e.resolved_path.ends_with("wt_a"));
        assert!(
            wt_a_entry.is_some(),
            "Worktree wt_a should be listed. Entries: {entries:?}"
        );
        assert_entry_properties(&entries, "wt_a", "GitWorktree", "[bare_container] wt_a");

        // Expected entries: bare_container, wt_a = 2
        assert_eq!(
            entries.len(),
            2,
            "Expected 2 entries (container repo, worktree). Entries: {entries:?}"
        );
    }

    #[test]
    fn test_scan_git_worktree() {
        let temp_dir = tempdir().unwrap();
        let main_repo_path = temp_dir.path().join("main_repo");
        init_repo(&main_repo_path);

        let worktree_dir = temp_dir.path().join("worktrees");
        fs::create_dir(&worktree_dir).unwrap();
        let worktree_checkout_path = worktree_dir.join("wt1");

        add_worktree_to_standard_repo(&main_repo_path, "wt1", &worktree_checkout_path);

        let mut config = default_test_config();
        // Scan the directory containing the worktree
        config.search_paths = vec![worktree_dir.clone()];

        let scanner = DirectoryScanner::new(&config);
        let entries = scanner.scan();

        assert_eq!(
            entries.len(),
            1,
            "Should find one entry, the worktree. Entries: {entries:?}"
        );
        let entry = &entries[0];
        assert!(entry.resolved_path.ends_with("wt1"));
        assert_eq!(entry.display_name, "[main_repo] wt1");
        match &entry.entry_type {
            DirectoryType::GitWorktree {
                main_worktree_path, ..
            } => {
                assert_eq!(
                    *main_worktree_path,
                    fs::canonicalize(&main_repo_path).unwrap()
                );
            }
            _ => panic!("Expected GitWorktree, found {:?}", entry.entry_type),
        }
    }

    #[test]
    fn test_scan_bare_repo_with_linked_worktrees() {
        let temp_dir = tempdir().unwrap();
        let bare_repo_path = temp_dir.path().join("bare_repo.git");
        let bare_repo = init_bare_repo(&bare_repo_path);

        let worktrees_dir = temp_dir.path().join("worktrees_of_bare");
        fs::create_dir(&worktrees_dir).unwrap();
        let wt_a_path = worktrees_dir.join("wt_a");
        let wt_b_path = worktrees_dir.join("wt_b");

        add_worktree_to_bare(&bare_repo, "wt_a", &wt_a_path);
        add_worktree_to_bare(&bare_repo, "wt_b", &wt_b_path);

        let mut config = default_test_config();
        // Scan the parent directory which contains both the bare repo and the worktrees directory
        config.search_paths = vec![temp_dir.path().to_path_buf()];

        let scanner = DirectoryScanner::new(&config);
        let mut entries = scanner.scan();
        entries.sort_by(|a, b| a.resolved_path.cmp(&b.resolved_path));

        // Expected entries found by WalkDir:
        // - bare_repo.git (GitRepository, lists wt_a, wt_b)
        // - worktrees_of_bare (Plain, skipped by container check)
        // - wt_a (GitWorktree, linked to bare_repo.git)
        // - wt_b (GitWorktree, linked to bare_repo.git)
        // Deduplication ensures wt_a and wt_b are listed once.

        // Assuming bare repo is NOT detected as exclusive container in this setup:
        let bare_repo_entry = entries
            .iter()
            .find(|e| e.resolved_path.ends_with("bare_repo.git"));
        assert!(
            bare_repo_entry.is_some(),
            "Bare repo itself should be listed if not exclusive container. Entries: {entries:?}"
        );
        assert_entry_properties(&entries, "bare_repo.git", "GitRepository", "bare_repo.git");

        let wt_a_entry = entries
            .iter()
            .find(|e| e.resolved_path.ends_with("wt_a"))
            .expect("wt_a not found");
        assert_eq!(wt_a_entry.display_name, "[bare_repo.git] wt_a");
        match &wt_a_entry.entry_type {
            DirectoryType::GitWorktree {
                main_worktree_path, ..
            } => {
                // For a bare repo, main_worktree_path points to the bare repo itself.
                assert_eq!(
                    *main_worktree_path,
                    fs::canonicalize(&bare_repo_path).unwrap()
                );
            }
            _ => panic!(
                "wt_a Expected GitWorktree, found {:?}",
                wt_a_entry.entry_type
            ),
        }

        let wt_b_entry = entries
            .iter()
            .find(|e| e.resolved_path.ends_with("wt_b"))
            .expect("wt_b not found");
        assert_eq!(wt_b_entry.display_name, "[bare_repo.git] wt_b");
        match &wt_b_entry.entry_type {
            DirectoryType::GitWorktree {
                main_worktree_path, ..
            } => {
                assert_eq!(
                    *main_worktree_path,
                    fs::canonicalize(&bare_repo_path).unwrap()
                );
            }
            _ => panic!(
                "wt_b Expected GitWorktree, found {:?}",
                wt_b_entry.entry_type
            ),
        }

        // Check that worktrees_of_bare (the containing directory) is skipped
        let worktrees_of_bare_entry = entries
            .iter()
            .find(|e| e.resolved_path.ends_with("worktrees_of_bare"));
        assert!(
            worktrees_of_bare_entry.is_none(),
            "worktrees_of_bare directory should be skipped. Entries: {entries:?}"
        );

        // Ensure no duplicates for worktrees
        let wt_a_count = entries
            .iter()
            .filter(|e| e.resolved_path.ends_with("wt_a"))
            .count();
        assert_eq!(wt_a_count, 1, "wt_a should appear exactly once");
        let wt_b_count = entries
            .iter()
            .filter(|e| e.resolved_path.ends_with("wt_b"))
            .count();
        assert_eq!(wt_b_count, 1, "wt_b should appear exactly once");

        // Total entries: bare_repo.git, wt_a, wt_b = 3
        assert_eq!(
            entries.len(),
            3,
            "Expected 3 entries: bare repo and two worktrees. Entries: {:?}",
            &entries
        );
    }

    #[test]
    fn test_scan_deduplication_of_paths() {
        let temp_dir = tempdir().unwrap();
        let project_a_path = temp_dir.path().join("project_a");
        fs::create_dir(&project_a_path).unwrap();

        let mut config = default_test_config();
        // Add the same path multiple times via different routes
        config.search_paths = vec![temp_dir.path().to_path_buf()]; // Finds project_a via WalkDir
        config.additional_paths = vec![project_a_path.clone()]; // Explicitly adds project_a

        let scanner = DirectoryScanner::new(&config);
        let entries = scanner.scan();

        // The Mutex<HashSet<PathBuf>> in `scan` should prevent the same resolved path
        // from being processed twice by `process_path_candidate`.
        assert_eq!(
            entries.len(),
            1,
            "Path added multiple ways should only appear once. Entries: {:?}",
            &entries
        );
        assert_entry_properties(&entries, "project_a", "Plain", "project_a");
    }

    #[test]
    fn test_search_path_is_git_repo_itself() {
        let temp_dir = tempdir().unwrap();
        let repo_path = temp_dir.path().join("my_repo");
        init_repo(&repo_path); // my_repo is a git repo
        fs::create_dir(repo_path.join("subdir")).unwrap(); // Add a subdir to it

        let mut config = default_test_config();
        // Add "my_repo" itself as a search path.
        // WalkDir starts *inside* the search path, so it will find "subdir".
        config.search_paths = vec![repo_path.clone()];

        let scanner = DirectoryScanner::new(&config);
        let entries = scanner.scan();

        // Expect "subdir" to be found as Plain. "my_repo" itself is the search root,
        // not an entry found *within* it by WalkDir starting there.
        assert_eq!(
            entries.len(),
            1,
            "Expected only subdir entry. Entries: {:?}",
            &entries
        );
        assert_entry_properties(&entries, "subdir", "Plain", "subdir");

        // If we want "my_repo" to be listed, it should be in additional_paths
        let mut config_additional = default_test_config();
        config_additional.additional_paths = vec![repo_path.clone()];
        let scanner_additional = DirectoryScanner::new(&config_additional);
        let entries_additional = scanner_additional.scan();
        assert_eq!(
            entries_additional.len(),
            1,
            "Expected my_repo entry from additional_paths. Entries: {:?}",
            &entries_additional
        );
        assert_entry_properties(&entries_additional, "my_repo", "GitRepository", "my_repo");
    }
}
