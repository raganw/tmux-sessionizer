
use git2::{Error, Repository};
use std::path::{Path, PathBuf};
use tracing::{debug, error, warn};

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
        let git_dir = get_git_dir_path(dir.path()).unwrap();
        assert_eq!(git_dir, dir.path().join(".git/")); // Note: repo.path() often includes a trailing slash
    }

    #[test]
    fn test_get_git_dir_path_bare() {
        let dir = tempdir().unwrap();
        init_bare_repo(dir.path());
        let git_dir = get_git_dir_path(dir.path()).unwrap();
        assert_eq!(git_dir, dir.path().join("")); // For bare repo, path is the repo itself. repo.path() might add /
                                                 // Let's check if it's the same as the repo dir
        assert_eq!(fs::canonicalize(git_dir).unwrap(), fs::canonicalize(dir.path()).unwrap());
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
}
