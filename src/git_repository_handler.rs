use crate::error::Result;
use git2::{Error as Git2Error, Repository};
use std::path::{Path, PathBuf};
use tracing::{Level, debug, error, span, warn};

/// Checks if the given path is a Git repository.
/// This could be a plain repository (with a .git subdirectory) or a bare repository.
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

/// Represents a Git worktree with its name and path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Worktree {
    pub name: String,
    pub path: PathBuf,
}

/// Lists all linked worktrees for the given repository.
/// The `repo_path` can be the path to the main working directory, a bare repository, or any of its linked worktrees.
/// This function does NOT list the main worktree itself, only those added via `git worktree add`.
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

/// Determines the canonical path of the main repository entity.
/// If `path_in_repo` is part of a non-bare repository, this returns the path to its working directory.
/// If `path_in_repo` is part of a bare repository, this returns the path to the bare repository itself.
/// This works whether `path_in_repo` is in the main worktree or a linked worktree.
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
mod tests {
    use super::*;
    use git2::WorktreeAddOptions;
    use std::fs::{self};
    use tempfile::tempdir;

    // Helper to initialize a standard git repo
    fn init_repo(path: &Path) -> Repository {
        Repository::init(path).expect("Failed to init repo")
    }

    // Helper to initialize a bare git repo
    fn init_bare_repo(path: &Path) -> Repository {
        Repository::init_bare(path).expect("Failed to init bare repo")
    }

    // Helper to add a worktree to a bare repo
    fn add_worktree_to_bare(
        bare_repo: &Repository,
        worktree_name: &str,
        worktree_path: &Path,
        branch_name: &str, // Branch must exist
    ) -> git2::Worktree {
        let mut opts = WorktreeAddOptions::new();
        let reference = bare_repo.find_reference(&format!("refs/heads/{branch_name}")).unwrap();
        opts.reference(Some(&reference));
        bare_repo.worktree(worktree_name, worktree_path, Some(&opts)).expect("Failed to add worktree to bare repo")
    }

    #[test]
    fn test_is_git_repository_standard() {
        let dir = tempdir().unwrap();
        init_repo(dir.path());
        assert!(is_git_repository(dir.path()));
    }

    #[test]
    fn test_is_git_repository_bare() {
        let dir = tempdir().unwrap();
        init_bare_repo(dir.path());
        assert!(is_git_repository(dir.path()));
    }

    #[test]
    fn test_is_git_repository_not_a_repo() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("subdir")).unwrap();
        assert!(!is_git_repository(dir.path().join("subdir").as_path()));
    }

    #[test]
    fn test_list_linked_worktrees_empty() {
        let main_repo_dir = tempdir().unwrap();
        init_repo(main_repo_dir.path());

        let worktrees = list_linked_worktrees(main_repo_dir.path()).unwrap();
        assert!(
            worktrees.is_empty(),
            "Should have no linked worktrees initially"
        );
    }

    #[test]
    fn test_list_linked_worktrees_with_one_worktree() {
        let main_repo_dir = tempdir().unwrap();
        let repo = init_repo(main_repo_dir.path());

        // Create an initial commit. This also sets up HEAD on the default branch (e.g., main/master).
        let signature = git2::Signature::now("Test User", "test@example.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let commit_oid = repo
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                "Initial commit",
                &tree,
                &[],
            )
            .expect("Failed to create initial commit");
        let commit = repo.find_commit(commit_oid).unwrap();

        // Main repo is on default branch (e.g. main/master)
        // Create a new branch for the worktree to checkout
        let worktree_branch_name = "worktree-specific-branch";
        repo.branch(worktree_branch_name, &commit, false) // false = no force
            .expect("Failed to create branch for worktree");

        // Use a subdirectory within a tempdir for the worktree path
        let base_wt_temp_dir = tempdir().unwrap();
        let wt_path = base_wt_temp_dir.path().join("my_worktree_dir"); // Path for the new worktree, does not exist yet
        let wt_name = "feature-branch";

        let mut opts = WorktreeAddOptions::new();
        let worktree_specific_ref = repo
            .find_reference(&format!("refs/heads/{worktree_branch_name}"))
            .unwrap();
        opts.reference(Some(&worktree_specific_ref));

        // git2 will create wt_path
        let _git2_worktree = repo.worktree(wt_name, &wt_path, Some(&opts)).unwrap();

        let worktrees = list_linked_worktrees(main_repo_dir.path()).unwrap();
        assert_eq!(worktrees.len(), 1);
        assert_eq!(worktrees[0].name, wt_name);
        assert_eq!(
            fs::canonicalize(&worktrees[0].path).unwrap(),
            fs::canonicalize(&wt_path).unwrap()
        );

        let worktrees_from_wt = list_linked_worktrees(&wt_path).unwrap();
        assert_eq!(worktrees_from_wt.len(), 1);
        assert_eq!(worktrees_from_wt[0].name, wt_name);
        assert_eq!(
            fs::canonicalize(&worktrees_from_wt[0].path).unwrap(),
            fs::canonicalize(&wt_path).unwrap()
        );
    }

    #[test]
    fn test_list_linked_worktrees_from_bare_repo() {
        let bare_repo_dir = tempdir().unwrap();
        let bare_repo = init_bare_repo(bare_repo_dir.path());

        // Bare repos need a commit before adding worktrees
        let signature = git2::Signature::now("Test User", "test@example.com").unwrap();
        let tree_id = {
            let mut index = bare_repo.index().unwrap(); // Create an empty index for the bare repo
            index.write_tree().unwrap() // Write it as a tree
        };
        let tree = bare_repo.find_tree(tree_id).unwrap();
        let initial_commit_oid = bare_repo
            .commit(
                None, // No HEAD in bare repo initially
                &signature,
                &signature,
                "Initial commit for bare repo",
                &tree,
                &[], // No parents
            )
            .expect("Failed to create initial commit in bare repo");
        let initial_commit = bare_repo.find_commit(initial_commit_oid).unwrap();

        // Create a branch pointing to the initial commit
        let branch_name = "main";
        bare_repo.branch(branch_name, &initial_commit, false).unwrap();

        // Add a worktree
        let base_wt_temp_dir = tempdir().unwrap();
        let wt_path = base_wt_temp_dir.path().join("bare_worktree");
        let wt_name = "bare-feature";
        add_worktree_to_bare(&bare_repo, wt_name, &wt_path, branch_name);

        // List worktrees from the bare repo path
        let worktrees = list_linked_worktrees(bare_repo_dir.path()).unwrap();
        assert_eq!(worktrees.len(), 1);
        assert_eq!(worktrees[0].name, wt_name);
        assert_eq!(
            fs::canonicalize(&worktrees[0].path).unwrap(),
            fs::canonicalize(&wt_path).unwrap()
        );
    }


    #[test]
    fn test_get_main_repository_path_for_standard_repo() {
        let repo_dir = tempdir().unwrap();
        init_repo(repo_dir.path());
        let main_path = get_main_repository_path(repo_dir.path()).unwrap();
        assert_eq!(main_path, fs::canonicalize(repo_dir.path()).unwrap());
    }

    #[test]
    fn test_get_main_repository_path_for_bare_repo() {
        let bare_repo_dir = tempdir().unwrap();
        init_bare_repo(bare_repo_dir.path());
        let main_path = get_main_repository_path(bare_repo_dir.path()).unwrap();
        assert_eq!(main_path, fs::canonicalize(bare_repo_dir.path()).unwrap());
    }

    #[test]
    fn test_get_main_repository_path_for_linked_worktree() {
        let main_repo_dir = tempdir().unwrap();
        let repo = init_repo(main_repo_dir.path());

        let signature = git2::Signature::now("Test User", "test@example.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let commit_oid = repo
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                "Initial commit",
                &tree,
                &[],
            )
            .expect("Failed to create initial commit");
        let commit = repo.find_commit(commit_oid).unwrap();

        let worktree_branch_name = "another-wt-branch";
        repo.branch(worktree_branch_name, &commit, false)
            .expect("Failed to create branch for worktree");

        let base_wt_temp_dir = tempdir().unwrap();
        let wt_path = base_wt_temp_dir.path().join("another_worktree_dir"); // Path for the new worktree
        let wt_name = "linked-feature";

        let mut opts = WorktreeAddOptions::new();
        let worktree_specific_ref = repo
            .find_reference(&format!("refs/heads/{worktree_branch_name}"))
            .unwrap();
        opts.reference(Some(&worktree_specific_ref));
        // git2 will create wt_path
        repo.worktree(wt_name, &wt_path, Some(&opts)).unwrap();

        let main_path_from_worktree = get_main_repository_path(&wt_path).unwrap();
        assert_eq!(
            main_path_from_worktree,
            fs::canonicalize(main_repo_dir.path()).unwrap(),
            "Main repo path from worktree should be the original main repo's path"
        );
    }

    #[test]
    fn test_get_main_repository_path_for_worktree_of_bare_repo() {
        let bare_repo_dir = tempdir().unwrap();
        let bare_repo = init_bare_repo(bare_repo_dir.path());

        // Bare repos need a commit before adding worktrees
        let signature = git2::Signature::now("Test User", "test@example.com").unwrap();
        let tree_id = {
            let mut index = bare_repo.index().unwrap();
            index.write_tree().unwrap()
        };
        let tree = bare_repo.find_tree(tree_id).unwrap();
        let initial_commit_oid = bare_repo
            .commit(
                None, &signature, &signature, "Initial commit for bare repo", &tree, &[],
            )
            .expect("Failed to create initial commit in bare repo");
        let initial_commit = bare_repo.find_commit(initial_commit_oid).unwrap();

        // Create a branch pointing to the initial commit
        let branch_name = "main";
        bare_repo.branch(branch_name, &initial_commit, false).unwrap();

        // Add a worktree
        let base_wt_temp_dir = tempdir().unwrap();
        let wt_path = base_wt_temp_dir.path().join("bare_worktree_for_main_path_test");
        let wt_name = "bare-feature-main-path";
        add_worktree_to_bare(&bare_repo, wt_name, &wt_path, branch_name);

        // Get main repo path from the worktree path
        let main_path_from_worktree = get_main_repository_path(&wt_path).unwrap();

        // The main path should be the canonical path of the bare repository itself
        assert_eq!(
            main_path_from_worktree,
            fs::canonicalize(bare_repo_dir.path()).unwrap(),
            "Main repo path from worktree of bare repo should be the bare repo's path"
        );
    }
}
