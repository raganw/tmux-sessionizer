//! Provides functions for interacting with Git repositories using the `git2` crate.
//!
//! This module includes functionality to:
//! - Check if a directory is a Git repository (standard or bare).
//! - List linked Git worktrees for a given repository path.
//! - Determine the main repository path (working directory for standard repos, path for bare repos)
//!   from any path within the repository or its worktrees.

use crate::error::Result;
use git2::{Error as Git2Error, Repository};
use std::path::{Path, PathBuf};
use tracing::{Level, debug, error, span, warn};

/// Checks if the given path corresponds to a Git repository.
///
/// This function attempts to open the path as a Git repository. It handles both
/// standard repositories (with a `.git` subdirectory) and bare repositories.
///
/// # Arguments
///
/// * `path` - The filesystem path to check.
///
/// # Returns
///
/// `true` if the path is a Git repository, `false` otherwise.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use tmux_sessionizer::git_repository_handler::is_git_repository;
///
/// // Assume "/path/to/my_repo" contains a valid Git repository
/// assert!(is_git_repository(Path::new("/path/to/my_repo")));
///
/// // Assume "/path/to/not_a_repo" is just a regular directory
/// assert!(!is_git_repository(Path::new("/path/to/not_a_repo")));
/// ```
pub fn is_git_repository(path: &Path) -> bool {
    match Repository::open(path) {
        Ok(_) => {
            debug!(path = %path.display(), "Path is a Git repository");
            true
        }
        Err(e) => {
            // Not every non-repo path is an error worth logging loudly,
            // but for debugging discovery, it can be useful.
            // Error code -3 (NotFound) is common for non-repo paths.
            if e.code() != git2::ErrorCode::NotFound || e.class() != git2::ErrorClass::Repository {
                warn!(path = %path.display(), error = %e, "Failed to open path as Git repository, assuming not a repo");
            } else {
                debug!(path = %path.display(), "Path is not a Git repository (standard check)");
            }
            false
        }
    }
}

/// Represents a Git worktree, containing its name and filesystem path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Worktree {
    /// The name of the worktree (e.g., "feature-branch").
    pub name: String,
    /// The absolute path to the worktree's root directory.
    pub path: PathBuf,
}

/// Lists all linked worktrees associated with a Git repository.
///
/// This function opens the Git repository located at `repo_path` (which can be the main
/// working directory, a bare repository path, or the path to any linked worktree) and
/// retrieves a list of all *other* linked worktrees.
///
/// Note: This function does *not* include the main worktree (if one exists) in the returned list.
/// It only lists worktrees added using `git worktree add`.
///
/// # Arguments
///
/// * `repo_path` - A path within any part of the Git repository structure (main worktree,
///   bare repo, or a linked worktree).
///
/// # Returns
///
/// A `Result` containing a `Vec<Worktree>` on success. Each `Worktree` struct contains the name
/// and absolute path of a linked worktree. Returns an error if the repository cannot be opened
/// or if there's an issue querying worktrees.
///
/// # Errors
///
/// This function can return errors originating from the `git2` library, such as:
/// - Repository not found at the given path.
/// - Corrupted repository data.
/// - Permission errors.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use tmux_sessionizer::git_repository_handler::list_linked_worktrees;
///
/// // Assume "/path/to/my_repo" is a Git repo with a linked worktree named "dev"
/// // located at "/path/to/my_repo_dev"
/// match list_linked_worktrees(Path::new("/path/to/my_repo")) {
///     Ok(worktrees) => {
///         assert_eq!(worktrees.len(), 1);
///         assert_eq!(worktrees[0].name, "dev");
///         assert_eq!(worktrees[0].path.to_str().unwrap(), "/path/to/my_repo_dev");
///     }
///     Err(e) => eprintln!("Error listing worktrees: {}", e),
/// }
/// ```
pub fn list_linked_worktrees(repo_path: &Path) -> Result<Vec<Worktree>> {
    let list_span = span!(Level::DEBUG, "list_linked_worktrees", repo_path = %repo_path.display());
    let _enter = list_span.enter();

    let repo = Repository::open(repo_path)?;

    let worktrees = repo.worktrees()?;
    debug!(
        count = worktrees.len(),
        "Found linked worktrees (raw count from git2)"
    );

    let mut result = Vec::new();
    for wt_name_bytes in &worktrees {
        if let Some(name_str) = wt_name_bytes {
            let name = name_str.to_string();
            debug!(worktree_name = %name, "Processing worktree entry from list");

            match repo.find_worktree(&name) {
                Ok(git2_worktree) => {
                    let wt_path = git2_worktree.path().to_path_buf();
                    debug!(name = %name, path = %wt_path.display(), "Found details for linked worktree");
                    result.push(Worktree {
                        name,
                        path: wt_path,
                    });
                }
                Err(e) => {
                    warn!(name = %name, error = %e, "Failed to find details for listed worktree, skipping");
                }
            }
        } else {
            warn!("Found a worktree with a non-UTF8 name, skipping");
        }
    }
    debug!(
        collected_count = result.len(),
        "Successfully collected linked worktree details"
    );
    Ok(result)
}

/// Determines the canonical path representing the "main" entity of a Git repository.
///
/// Given a path that resides anywhere within a Git repository structure (main worktree,
/// linked worktree, or the `.git` directory itself, including bare repositories), this function
/// identifies and returns the canonical path of the primary repository entity.
///
/// - For a **standard (non-bare) repository**, this is the canonical path to the **main working directory**
///   (the directory containing the `.git` folder, or the directory the bare repo was cloned into initially).
/// - For a **bare repository**, this is the canonical path to the **bare repository directory itself**
///   (e.g., `/path/to/my_repo.git`).
///
/// This function correctly resolves the main path even when `path_in_repo` points to a linked worktree.
///
/// # Arguments
///
/// * `path_in_repo` - A path known to be inside a Git repository structure.
///
/// # Returns
///
/// A `Result` containing the canonical `PathBuf` of the main repository entity on success.
/// Returns an error if the repository cannot be opened, if the path structure is unexpected,
/// or if canonicalization fails.
///
/// # Errors
///
/// This function can return errors from:
/// - `git2::Repository::open` if the path doesn't belong to a valid repository.
/// - Filesystem operations like `std::fs::canonicalize` if the determined path doesn't exist or
///   there are permission issues.
/// - Logic errors if the repository structure is inconsistent (e.g., a worktree's common dir
///   doesn't have a parent for a non-bare repo).
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use tmux_sessionizer::git_repository_handler::get_main_repository_path;
///
/// // Assume standard repo at "/projects/my_app" with worktree at "/projects/my_app_dev"
/// let main_repo_path = Path::new("/projects/my_app");
/// let worktree_path = Path::new("/projects/my_app_dev");
///
/// // Get main path from the main repo directory
/// assert_eq!(get_main_repository_path(main_repo_path).unwrap(), std::fs::canonicalize(main_repo_path).unwrap());
///
/// // Get main path from within the main repo
/// assert_eq!(get_main_repository_path(&main_repo_path.join("src")).unwrap(), std::fs::canonicalize(main_repo_path).unwrap());
///
/// // Get main path from the worktree directory
/// assert_eq!(get_main_repository_path(worktree_path).unwrap(), std::fs::canonicalize(main_repo_path).unwrap());
///
/// // Assume bare repo at "/srv/git/my_bare_repo.git" with worktree at "/home/user/dev/my_bare_repo_wt"
/// let bare_repo_path = Path::new("/srv/git/my_bare_repo.git");
/// let bare_worktree_path = Path::new("/home/user/dev/my_bare_repo_wt");
///
/// // Get main path from the bare repo path
/// assert_eq!(get_main_repository_path(bare_repo_path).unwrap(), std::fs::canonicalize(bare_repo_path).unwrap());
///
/// // Get main path from the worktree of the bare repo
/// assert_eq!(get_main_repository_path(bare_worktree_path).unwrap(), std::fs::canonicalize(bare_repo_path).unwrap());
/// ```
pub fn get_main_repository_path(path_in_repo: &Path) -> Result<PathBuf> {
    let path_span =
        span!(Level::DEBUG, "get_main_repository_path", path_in_repo = %path_in_repo.display());
    let _enter = path_span.enter();

    let repo = Repository::open(path_in_repo)?;

    let main_path_candidate = if repo.is_worktree() {
        // If it's a worktree, repo.commondir() is the .git dir of the main repository.
        // So, its parent is the working directory of the main repository.
        // However, if the main repository is bare, repo.commondir() IS the main repository path.
        let common_dir = repo.commondir();
        debug!(path_in_repo = %path_in_repo.display(), worktree_commondir = %common_dir.display(), "Determining main path for worktree");
        // Check if the repository at common_dir is bare
        match Repository::open(common_dir) {
            Ok(main_repo_at_commondir) => {
                if main_repo_at_commondir.is_bare() {
                    common_dir.to_path_buf() // If main repo is bare, its path is common_dir
                } else {
                    // If main repo is not bare, its workdir is common_dir.parent()
                    common_dir.parent()
                        .ok_or_else(|| Git2Error::from_str("Worktree's common directory (for non-bare main repo) has no parent"))?
                        .to_path_buf()
                }
            }
            Err(e) => {
                error!(commondir_path = %common_dir.display(), error = %e, "Failed to open repository at worktree's common_dir to determine if bare");
                return Err(e.into());
            }
        }
    } else if repo.is_bare() {
        debug!(path_in_repo = %path_in_repo.display(), "Repository is bare, main path is repo.path()");
        repo.path().to_path_buf() // For a bare repo, its own path is the main repository path.
    } else {
        // For a non-bare, non-worktree repository, its workdir is the main repository path.
        debug!(path_in_repo = %path_in_repo.display(), "Repository is non-bare, non-worktree, main path is repo.workdir()");
        repo.workdir()
            .ok_or_else(|| Git2Error::from_str("Non-bare, non-worktree repository has no workdir"))?
            .to_path_buf()
    };

    debug!(candidate_main_path = %main_path_candidate.display(), "Candidate main repository path determined");

    // Canonicalize the determined path.
    // Note: fs::canonicalize might fail if the path doesn't exist, but at this stage, it should.
    let canonical_path = std::fs::canonicalize(&main_path_candidate)?;
    debug!(path = %canonical_path.display(), "Canonicalized main repository path");
    Ok(canonical_path)
}

#[cfg(test)]
mod tests;
