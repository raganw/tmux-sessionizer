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
mod tests;
