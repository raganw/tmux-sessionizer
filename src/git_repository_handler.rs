
use git2::{Error, Repository, WorktreeAddOptions}; // Added WorktreeAddOptions for tests
use std::path::{Path, PathBuf};
use tracing::{debug, error, span, warn, Level}; // Added span and Level

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

/// Gets the path to the .git directory for a repository.
/// For a normal repository, this is `path/.git/`.
/// For a bare repository, this is `path/`.
/// For a worktree, this points to the .git file which then points to the actual gitdir in the parent repo.
pub fn get_git_dir_path(repo_path: &Path) -> Result<PathBuf, Error> {
    let repo = Repository::open(repo_path)?;
    let git_dir = repo.path().to_path_buf();
    debug!(repo_path = %repo_path.display(), git_dir = %git_dir.display(), "Found .git directory");
    Ok(git_dir)
}

/// Checks if a Git repository at the given path is a bare repository.
pub fn is_bare_repository(repo_path: &Path) -> Result<bool, Error> {
    match Repository::open(repo_path) {
        Ok(repo) => {
            let is_bare = repo.is_bare();
            debug!(path = %repo_path.display(), is_bare, "Checked if repository is bare");
            Ok(is_bare)
        }
        Err(e) => {
            error!(path = %repo_path.display(), error = %e, "Failed to open repository to check if bare");
            Err(e)
        }
    }
}

// Additional helper that might be useful later, checks if it's a worktree's .git file
// or a common .git directory.
// A .git file in a worktree typically contains: `gitdir: /path/to/parent/repo/.git/worktrees/worktree-name`
pub fn is_worktree_git_dir(git_dir_path: &Path) -> bool {
    if git_dir_path.is_file() {
        // A .git file (not directory) usually indicates a worktree or submodule.
        // For worktrees, it contains a `gitdir:` line.
        // For submodules, it contains a `gitdir:` line pointing into the parent's .git/modules.
        // We are primarily interested in worktrees here.
        if let Ok(content) = std::fs::read_to_string(git_dir_path) {
            if content.trim().starts_with("gitdir:") {
                debug!(path = %git_dir_path.display(), "Path is a .git file (likely worktree or submodule)");
                return true; // Could be a worktree or submodule .git file
            }
        }
    }
    debug!(path = %git_dir_path.display(), "Path is not a .git file (likely a .git directory or not a git dir)");
    false
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
pub fn list_linked_worktrees(repo_path: &Path) -> Result<Vec<Worktree>, Error> {
    let list_span = span!(Level::DEBUG, "list_linked_worktrees", repo_path = %repo_path.display());
    let _enter = list_span.enter();

    let repo = match Repository::open(repo_path) {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "Failed to open repository to list worktrees");
            return Err(e);
        }
    };

    let worktrees = repo.worktrees()?;
    debug!(count = worktrees.len(), "Found linked worktrees (raw count from git2)");

    let mut result = Vec::new();
    for wt_name_bytes in worktrees.iter() {
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
    debug!(collected_count = result.len(), "Successfully collected linked worktree details");
    Ok(result)
}

/// Determines the canonical path of the main repository entity.
/// If `path_in_repo` is part of a non-bare repository, this returns the path to its working directory.
/// If `path_in_repo` is part of a bare repository, this returns the path to the bare repository itself.
/// This works whether `path_in_repo` is in the main worktree or a linked worktree.
pub fn get_main_repository_path(path_in_repo: &Path) -> Result<PathBuf, Error> {
    let path_span = span!(Level::DEBUG, "get_main_repository_path", path_in_repo = %path_in_repo.display());
    let _enter = path_span.enter();

    let repo = match Repository::open(path_in_repo) {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "Failed to open repository to find main repository path for {}", path_in_repo.display());
            return Err(e);
        }
    };

    let main_path_candidate = if repo.is_worktree() {
        // A worktree's .git file points to <main_repo_path>/.git/worktrees/<worktree_name>.
        // repo.commondir() for a worktree gives <main_repo_path>/.git/worktrees/<worktree_name>.
        // The parent of this is <main_repo_path>/.git/worktrees.
        // The parent of *that* is <main_repo_path>/.git.
        // The parent of *that* is <main_repo_path>.

        let common_dir = repo.commondir(); // e.g., /path/to/main/.git/worktrees/wt_name
        common_dir.parent() // e.g., /path/to/main/.git/worktrees
            .and_then(|p| p.parent()) // e.g., /path/to/main/.git
            .and_then(|p| p.parent()) // e.g., /path/to/main
            .ok_or_else(|| Error::from_str("Could not determine main repository path from worktree commondir structure"))?
            .to_path_buf()

    } else if repo.is_bare() {
        repo.path().to_path_buf() // For a bare repo, its own path is the main repository path.
    } else {
        // For a non-bare, non-worktree repository, its workdir is the main repository path.
        repo.workdir().ok_or_else(|| Error::from_str("Non-bare, non-worktree repository has no workdir"))?.to_path_buf()
    };
    
    debug!(candidate_main_path = %main_path_candidate.display(), "Candidate main repository path determined");

    match std::fs::canonicalize(&main_path_candidate) {
        Ok(p) => {
            debug!(path = %p.display(), "Canonicalized main repository path");
            Ok(p)
        }
        Err(e) => {
            error!(path = %main_path_candidate.display(), error = %e, "Failed to canonicalize main repository path, using non-canonical");
            Ok(main_path_candidate)
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::tempdir;

    // Helper to initialize a standard git repo
    fn init_repo(path: &Path) -> Repository {
        Repository::init(path).expect("Failed to init repo")
    }

    // Helper to initialize a bare git repo
    fn init_bare_repo(path: &Path) -> Repository {
        Repository::init_bare(path).expect("Failed to init bare repo")
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
    fn test_get_git_dir_path_standard() {
        let dir = tempdir().unwrap();
        init_repo(dir.path());
        let git_dir_from_func = get_git_dir_path(dir.path()).unwrap();
        let expected_git_dir = dir.path().join(".git/"); // repo.path() for standard repo includes .git/

        let canon_git_dir = fs::canonicalize(git_dir_from_func).unwrap();
        let canon_expected_dir = fs::canonicalize(expected_git_dir).unwrap();
        
        // Normalize by removing trailing slash if present for comparison
        let norm_canon_git_dir = PathBuf::from(canon_git_dir.to_string_lossy().trim_end_matches('/'));
        let norm_canon_expected_dir = PathBuf::from(canon_expected_dir.to_string_lossy().trim_end_matches('/'));

        assert_eq!(norm_canon_git_dir, norm_canon_expected_dir);
    }

    #[test]
    fn test_get_git_dir_path_bare() {
        let dir = tempdir().unwrap();
        init_bare_repo(dir.path());
        let git_dir_from_func = get_git_dir_path(dir.path()).unwrap(); // This is repo.path()

        let canon_git_dir = fs::canonicalize(git_dir_from_func).unwrap();
        let canon_temp_dir = fs::canonicalize(dir.path()).unwrap();

        // Normalize by removing trailing slash if present for comparison
        let norm_canon_git_dir = PathBuf::from(canon_git_dir.to_string_lossy().trim_end_matches('/'));
        let norm_canon_temp_dir = PathBuf::from(canon_temp_dir.to_string_lossy().trim_end_matches('/'));

        assert_eq!(norm_canon_git_dir, norm_canon_temp_dir);
    }


    #[test]
    fn test_is_bare_repository_standard() {
        let dir = tempdir().unwrap();
        init_repo(dir.path());
        assert!(!is_bare_repository(dir.path()).unwrap());
    }

    #[test]
    fn test_is_bare_repository_bare() {
        let dir = tempdir().unwrap();
        init_bare_repo(dir.path());
        assert!(is_bare_repository(dir.path()).unwrap());
    }

    #[test]
    fn test_is_worktree_git_dir_true() {
        let dir = tempdir().unwrap();
        let git_file_path = dir.path().join(".git");
        let mut file = File::create(&git_file_path).unwrap();
        writeln!(file, "gitdir: /path/to/real/git/dir").unwrap();
        assert!(is_worktree_git_dir(&git_file_path));
    }

    #[test]
    fn test_is_worktree_git_dir_false_for_dir() {
        let dir = tempdir().unwrap();
        let git_dir_path = dir.path().join(".git");
        fs::create_dir(&git_dir_path).unwrap();
        assert!(!is_worktree_git_dir(&git_dir_path));
    }

    #[test]
    fn test_is_worktree_git_dir_false_for_unrelated_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("some_file.txt");
        File::create(&file_path).unwrap();
        assert!(!is_worktree_git_dir(&file_path));
    }

    #[test]
    fn test_list_linked_worktrees_empty() {
        let main_repo_dir = tempdir().unwrap();
        init_repo(main_repo_dir.path());

        let worktrees = list_linked_worktrees(main_repo_dir.path()).unwrap();
        assert!(worktrees.is_empty(), "Should have no linked worktrees initially");
    }

    #[test]
    fn test_list_linked_worktrees_with_one_worktree() {
        let main_repo_dir = tempdir().unwrap();
        let repo = init_repo(main_repo_dir.path());

        // Create a commit for the branch to be based on
        let signature = git2::Signature::now("Test User", "test@example.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let initial_commit_oid = repo.commit(Some("HEAD"), &signature, &signature, "Initial commit", &tree, &[]).unwrap();
        let initial_commit = repo.find_commit(initial_commit_oid).unwrap();
        repo.branch("main", &initial_commit, true).expect("Failed to create main branch");
        // Set HEAD to the new main branch to ensure it's not unborn
        repo.set_head("refs/heads/main").expect("Failed to set HEAD to main branch");
        
        let wt_dir = tempdir().unwrap();
        let wt_path = wt_dir.path(); // Path for the new worktree
        let wt_name = "feature-branch";

        // Add worktree using git2
        let mut opts = WorktreeAddOptions::new();
        let branch_ref = repo.find_reference("refs/heads/main").unwrap();
        opts.reference(Some(&branch_ref)); 
        
        // Need to ensure the path for the worktree is outside the main repo's temp dir
        // or use a relative path that makes sense. tempdir() creates unique paths.
        let _git2_worktree = repo.worktree(wt_name, wt_path, Some(&opts)).unwrap();
        
        let worktrees = list_linked_worktrees(main_repo_dir.path()).unwrap();
        assert_eq!(worktrees.len(), 1, "Should list one linked worktree");
        assert_eq!(worktrees[0].name, wt_name);
        // git2::Worktree::path() returns canonicalized path, so we should compare canonicalized
        assert_eq!(fs::canonicalize(&worktrees[0].path).unwrap(), fs::canonicalize(wt_path).unwrap());

        // Test listing from the worktree itself
        let worktrees_from_wt = list_linked_worktrees(wt_path).unwrap();
         assert_eq!(worktrees_from_wt.len(), 1, "Should list one linked worktree when called from worktree path");
         assert_eq!(worktrees_from_wt[0].name, wt_name);
         assert_eq!(fs::canonicalize(&worktrees_from_wt[0].path).unwrap(), fs::canonicalize(wt_path).unwrap());
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
        let initial_commit_oid = repo.commit(Some("HEAD"), &signature, &signature, "Initial commit", &tree, &[]).unwrap();
        let initial_commit = repo.find_commit(initial_commit_oid).unwrap();
        repo.branch("main", &initial_commit, true).expect("Failed to create main branch");
        repo.set_head("refs/heads/main").expect("Failed to set HEAD to main branch");

        let wt_dir = tempdir().unwrap(); // Separate temp dir for the worktree
        let wt_path = wt_dir.path();
        let wt_name = "linked-feature";
        
        let mut opts = WorktreeAddOptions::new();
        let branch_ref = repo.find_reference("refs/heads/main").unwrap();
        opts.reference(Some(&branch_ref));
        repo.worktree(wt_name, wt_path, Some(&opts)).unwrap();

        let main_path_from_worktree = get_main_repository_path(wt_path).unwrap();
        assert_eq!(main_path_from_worktree, fs::canonicalize(main_repo_dir.path()).unwrap(), 
            "Main repo path from worktree should be the original main repo's path");
    }
}
