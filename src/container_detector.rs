//! Detects special types of Git repository structures, specifically "worktree containers".
//!
//! This module provides functions to identify directories that act as containers
//! for Git worktrees, distinguishing between bare repositories that contain their
//! own worktrees and plain directories that exclusively contain worktrees from a
//! single main repository.

use crate::error::Result;
use crate::git_repository_handler;
use git2::Repository;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{Level, debug, span, warn};

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
/// Returns `Ok(true)` if it contains at least one qualifying worktree child, `Ok(false)` otherwise.
/// Returns `Err` if a fundamental operation (like reading the directory) fails.
pub fn is_bare_repo_worktree_exclusive_container(
    container_candidate_path: &Path,
    bare_repo: &Repository,
) -> Result<bool> {
    let container_check_span = span!(
        Level::DEBUG,
        "is_bare_repo_worktree_exclusive_container",
        path = %container_candidate_path.display()
    );
    let _enter = container_check_span.enter();

    let mut worktree_children_count = 0;

    let canonical_bare_repo_dotgit_path = fs::canonicalize(bare_repo.path())?;
    debug!(path = %container_candidate_path.display(), bare_repo_dotgit_path = %canonical_bare_repo_dotgit_path.display(), "Checking for bare repo exclusive container");

    for entry_result in fs::read_dir(container_candidate_path)? {
        let entry = entry_result?;
        let child_path = entry.path();
        let child_name = child_path.file_name().unwrap_or_default();

        if child_name == OsStr::new(".git") {
            debug!(child = %child_path.display(), "Ignoring .git entry in container.");
            continue;
        }

        let canonical_child_path = match fs::canonicalize(&child_path) {
            Ok(p) => p,
            Err(e) => {
                warn!(child = %child_path.display(), error = %e, "Could not canonicalize child path, skipping this entry.");
                continue; // Skip if this specific child cannot be canonicalized
            }
        };

        if canonical_child_path == canonical_bare_repo_dotgit_path {
            debug!(child = %child_path.display(), "Ignoring actual bare repo .git directory child.");
            continue;
        }

        let child_file_type = entry.file_type()?;

        if child_file_type.is_file() {
            debug!(child = %child_path.display(), "Child is a file, not a worktree. Continuing search for worktrees.");
            continue;
        }

        if child_file_type.is_dir() {
            match Repository::open(&canonical_child_path) {
                Ok(child_repo) => {
                    if child_repo.is_worktree() {
                        match git_repository_handler::get_main_repository_path(
                            &canonical_child_path,
                        ) {
                            Ok(wt_main_repo_path) => {
                                match fs::canonicalize(&wt_main_repo_path) {
                                    Ok(canonical_wt_main_repo_path) => {
                                        if canonical_wt_main_repo_path
                                            == canonical_bare_repo_dotgit_path
                                        {
                                            debug!(child_worktree = %canonical_child_path.display(), "Child is a qualifying worktree of this bare repo.");
                                            worktree_children_count += 1;
                                        } else {
                                            debug!(child_worktree = %canonical_child_path.display(), main_repo = %canonical_wt_main_repo_path.display(), expected_main_repo = %canonical_bare_repo_dotgit_path.display(), "Child worktree belongs to a different main repo.");
                                            // This is fine, just not *this* repo's worktree.
                                        }
                                    }
                                    Err(e) => {
                                        warn!(wt_main_repo_path = %wt_main_repo_path.display(), error = %e, "Failed to canonicalize worktree's main repo path. Skipping this worktree check.");
                                        // Continue, as this specific worktree cannot be fully verified.
                                    }
                                }
                            }
                            Err(e) => {
                                warn!(child_worktree = %canonical_child_path.display(), error = %e, "Failed to get main repository path for worktree child. Skipping this worktree check.");
                                // Continue, as this specific worktree cannot be fully verified.
                            }
                        }
                    } else {
                        debug!(child = %canonical_child_path.display(), "Child is a Git repository but not a worktree.");
                        // This is fine, just not a worktree.
                    }
                }
                Err(_) => {
                    debug!(child = %canonical_child_path.display(), "Child is not a Git repository.");
                    // This is fine, just not a git repo.
                }
            }
        } else {
            debug!(child = %child_path.display(), "Child is of unexpected type.");
            // This is fine, just not a directory.
        }
    }

    let is_container = worktree_children_count > 0;
    if is_container {
        debug!(path = %container_candidate_path.display(), worktree_count = worktree_children_count, "Path IS a bare repo worktree container (relaxed check).");
    } else {
        debug!(path = %container_candidate_path.display(), worktree_count = worktree_children_count, "Path is NOT a bare repo worktree container (relaxed check): no qualifying worktree children found.");
    }
    Ok(is_container)
}

/// Checks if the given path is a "worktree container".
/// A directory is considered a worktree container if all of its direct children are
/// directories, each of those is a Git worktree, and all those worktrees belong
/// to the same main repository. It must also contain at least one such worktree
/// and no other files or non-qualifying directories at its top level.
/// This function is for paths that are NOT git repositories themselves.
///
/// Returns `Ok(true)` if it's a worktree container, `Ok(false)` otherwise.
/// Returns `Err` if a fundamental operation (like reading the directory or canonicalizing paths) fails.
pub fn check_if_worktree_container(path_to_check: &Path) -> Result<bool> {
    let container_check_span = span!(
        Level::DEBUG,
        "check_if_worktree_container",
        path = %path_to_check.display()
    );
    let _enter = container_check_span.enter();

    let mut worktree_children_count = 0;
    let mut first_main_repo_path: Option<PathBuf> = None;
    let mut all_children_are_qualifying_worktrees = true;

    for entry_result in fs::read_dir(path_to_check)? {
        let entry = entry_result?;
        let child_path = entry.path();
        let child_file_type = entry.file_type()?;

        if child_file_type.is_file() {
            debug!(child = %child_path.display(), "Child is a file, parent not a worktree container.");
            all_children_are_qualifying_worktrees = false;
            break;
        }

        if child_file_type.is_dir() || child_file_type.is_symlink() {
            let canonical_child_path = fs::canonicalize(&child_path)?;
            if !canonical_child_path.is_dir() {
                // Check after canonicalization
                debug!(child = %child_path.display(), resolved = %canonical_child_path.display(), "Child resolved to non-directory, parent not a worktree container.");
                all_children_are_qualifying_worktrees = false;
                break;
            }

            if let Ok(repo) = Repository::open(&canonical_child_path) {
                if repo.is_worktree() {
                    // Use `?` to propagate error from get_main_repository_path
                    // If it errors, it means this child cannot be confirmed, so the "all children" criteria fails.
                    let main_repo_path_val = match git_repository_handler::get_main_repository_path(
                        &canonical_child_path,
                    ) {
                        Ok(p) => p,
                        Err(e) => {
                            warn!(child_worktree = %canonical_child_path.display(), error = %e, "Failed to get main repository path for worktree child. Not a valid container.");
                            all_children_are_qualifying_worktrees = false;
                            break;
                        }
                    };

                    let current_main_repo_path = fs::canonicalize(&main_repo_path_val)?;

                    if first_main_repo_path.is_none() {
                        first_main_repo_path = Some(current_main_repo_path.clone());
                        debug!(child_worktree = %canonical_child_path.display(), main_repo = %current_main_repo_path.display(), "First worktree found, setting common main repo path.");
                    } else if first_main_repo_path.as_ref() != Some(&current_main_repo_path) {
                        debug!(child_worktree = %canonical_child_path.display(), main_repo = %current_main_repo_path.display(), expected_main_repo = ?first_main_repo_path, "Worktree belongs to a different main repo, parent not a container.");
                        all_children_are_qualifying_worktrees = false;
                        break;
                    }
                    worktree_children_count += 1;
                } else {
                    debug!(child = %canonical_child_path.display(), "Child is a Git repository but not a worktree, parent not a container.");
                    all_children_are_qualifying_worktrees = false;
                    break;
                }
            } else {
                // Not a Git repository
                debug!(child = %canonical_child_path.display(), "Child is not a Git repository, parent not a worktree container.");
                all_children_are_qualifying_worktrees = false;
                break;
            }
        } else {
            // Not a dir, file, or symlink (should be rare)
            debug!(child = %child_path.display(), "Child is of unknown type, parent not a worktree container.");
            all_children_are_qualifying_worktrees = false;
            break;
        }
    }

    let is_container = all_children_are_qualifying_worktrees
        && worktree_children_count > 0
        && first_main_repo_path.is_some();

    if is_container {
        debug!(path = %path_to_check.display(), common_main_repo = ?first_main_repo_path, worktree_count = worktree_children_count, "Path IS a worktree container.");
    } else {
        debug!(path = %path_to_check.display(), all_children_ok = all_children_are_qualifying_worktrees, worktree_count = worktree_children_count, main_repo_found = first_main_repo_path.is_some(), "Path is NOT a worktree container.");
    }
    Ok(is_container)
}

#[cfg(test)]
mod tests;
