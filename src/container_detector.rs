use crate::git_repository_handler;
use git2::Repository;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, span, warn, Level};

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
pub fn is_bare_repo_worktree_exclusive_container(
    container_candidate_path: &Path,
    bare_repo: &Repository,
) -> bool {
    let container_check_span = span!(
        Level::DEBUG,
        "is_bare_repo_worktree_exclusive_container",
        path = %container_candidate_path.display()
    );
    let _enter = container_check_span.enter();

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

                        if child_name == OsStr::new(".git") {
                            debug!(child = %child_path.display(), "Ignoring .git entry in container.");
                            continue;
                        }

                        let canonical_child_path = match fs::canonicalize(&child_path) {
                            Ok(p) => p,
                            Err(e) => {
                                warn!(child = %child_path.display(), error = %e, "Could not canonicalize child path, assuming not a qualifying worktree container child.");
                                continue;
                            }
                        };

                        if canonical_child_path == canonical_bare_repo_dotgit_path {
                            debug!(child = %child_path.display(), "Ignoring actual bare repo .git directory child.");
                            continue;
                        }

                        let child_file_type = match entry.file_type() {
                            Ok(ft) => ft,
                            Err(e) => {
                                warn!(child = %child_path.display(), error = %e, "Could not get file type for child, assuming not a qualifying worktree container child.");
                                continue;
                            }
                        };

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
                                                            continue;
                                                        }
                                                    }
                                                    Err(e) => {
                                                        warn!(wt_main_repo_path = %wt_main_repo_path.display(), error = %e, "Failed to canonicalize worktree's main repo path.");
                                                        continue;
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                warn!(child_worktree = %canonical_child_path.display(), error = %e, "Failed to get main repository path for worktree child.");
                                                continue;
                                            }
                                        }
                                    } else {
                                        debug!(child = %canonical_child_path.display(), "Child is a Git repository but not a worktree.");
                                        continue;
                                    }
                                }
                                Err(_) => {
                                    debug!(child = %canonical_child_path.display(), "Child is not a Git repository.");
                                    continue;
                                }
                            }
                        } else {
                            debug!(child = %child_path.display(), "Child is of unexpected type.");
                            continue;
                        }
                    }
                    Err(e) => {
                        warn!(path = %container_candidate_path.display(), error = %e, "Error iterating directory entry.");
                        continue;
                    }
                }
            }
        }
        Err(e) => {
            warn!(path = %container_candidate_path.display(), error = %e, "Could not read directory to check for bare repo container status.");
            return false;
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

/// Checks if the given path is a "worktree container".
/// A directory is considered a worktree container if all of its direct children are
/// directories, each of those is a Git worktree, and all those worktrees belong
/// to the same main repository. It must also contain at least one such worktree
/// and no other files or non-qualifying directories at its top level.
/// This function is for paths that are NOT git repositories themselves.
///
/// Returns `true` if it's a worktree container, `false` otherwise.
pub fn check_if_worktree_container(path_to_check: &Path) -> bool {
    let container_check_span = span!(
        Level::DEBUG,
        "check_if_worktree_container",
        path = %path_to_check.display()
    );
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

                        if child_file_type.is_file() {
                            debug!(child = %child_path.display(), "Child is a file, parent not a worktree container.");
                            all_children_are_qualifying_worktrees = false;
                            break;
                        }

                        if child_file_type.is_dir() || child_file_type.is_symlink() {
                            match fs::canonicalize(&child_path) {
                                Ok(canonical_child_path) => {
                                    if !canonical_child_path.is_dir() {
                                        debug!(child = %child_path.display(), resolved = %canonical_child_path.display(), "Child resolved to non-directory, parent not a worktree container.");
                                        all_children_are_qualifying_worktrees = false;
                                        break;
                                    }

                                    match Repository::open(&canonical_child_path) {
                                        Ok(repo) => {
                                            if repo.is_worktree() {
                                                match git_repository_handler::get_main_repository_path(&canonical_child_path) {
                                                    Ok(main_repo_path_val) => {
                                                        let current_main_repo_path = match fs::canonicalize(&main_repo_path_val) {
                                                            Ok(p) => p,
                                                            Err(e) => {
                                                                warn!(path = %main_repo_path_val.display(), error = %e, "Failed to canonicalize main repo path for worktree child {}, using as-is.", canonical_child_path.display());
                                                                main_repo_path_val
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
                                                        worktree_children_count += 1;
                                                    }
                                                    Err(e) => {
                                                        warn!(child_worktree = %canonical_child_path.display(), error = %e, "Failed to get main repository path for worktree child.");
                                                        all_children_are_qualifying_worktrees = false;
                                                        break;
                                                    }
                                                }
                                            } else {
                                                debug!(child = %canonical_child_path.display(), "Child is a Git repository but not a worktree, parent not a container.");
                                                all_children_are_qualifying_worktrees = false;
                                                break;
                                            }
                                        }
                                        Err(_) => {
                                            debug!(child = %canonical_child_path.display(), "Child is not a Git repository, parent not a worktree container.");
                                            all_children_are_qualifying_worktrees = false;
                                            break;
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!(child = %child_path.display(), error = %e, "Could not canonicalize child path.");
                                    all_children_are_qualifying_worktrees = false;
                                    break;
                                }
                            }
                        } else {
                            debug!(child = %child_path.display(), "Child is of unknown type, parent not a worktree container.");
                            all_children_are_qualifying_worktrees = false;
                            break;
                        }
                    }
                    Err(e) => {
                        warn!(path = %path_to_check.display(), error = %e, "Error iterating directory entry.");
                        all_children_are_qualifying_worktrees = false;
                        break;
                    }
                }
            }
        }
        Err(e) => {
            warn!(path = %path_to_check.display(), error = %e, "Could not read directory to check for worktree container status.");
            return false;
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
    is_container
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{Repository, Signature, WorktreeAddOptions};
    use std::fs::{self, File};
    use std::path::Path;
    use tempfile::tempdir;

    // Helper to initialize a bare git repo (copied from directory_scanner tests)
    fn init_bare_repo(path: &Path) -> Repository {
        Repository::init_bare(path).expect("Failed to init bare repo")
    }

    // Helper to add a worktree to a bare repository (copied from directory_scanner tests)
    fn add_worktree_to_bare(
        bare_repo: &Repository,
        worktree_name: &str,
        worktree_path: &Path,
    ) -> Repository {
        if bare_repo.is_empty().unwrap_or(true) {
            let mut index = bare_repo.index().expect("Failed to get index for bare repo");
            let tree_id = index.write_tree().expect("Failed to write empty tree");
            let tree = bare_repo.find_tree(tree_id).expect("Failed to find tree");
            let sig = Signature::now("Test User", "test@example.com")
                .expect("Failed to create signature");
            bare_repo
                .commit(
                    Some("HEAD"),
                    &sig,
                    &sig,
                    "Initial commit",
                    &tree,
                    &[],
                )
                .expect("Failed to create initial commit in bare repo");
        }
        fs::create_dir_all(worktree_path.parent().unwrap())
            .expect("Failed to create parent for worktree path");
        let mut opts = WorktreeAddOptions::new();
        bare_repo
            .worktree(worktree_name, worktree_path, Some(&mut opts))
            .expect("Failed to add worktree");
        Repository::open(worktree_path).expect("Failed to open added worktree")
    }

    #[test]
    fn test_is_bare_repo_worktree_exclusive_container_true() {
        let base_dir = tempdir().unwrap();
        let container_path = base_dir.path().join("project_container");
        fs::create_dir(&container_path).unwrap();

        let bare_repo_actual_path = container_path.join("actual_bare.git");
        fs::create_dir(&bare_repo_actual_path).unwrap();
        let _bare_repo_git_object = init_bare_repo(&bare_repo_actual_path);

        fs::write(
            container_path.join(".git"),
            format!(
                "gitdir: {}",
                bare_repo_actual_path
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
            ),
        )
        .unwrap();

        let container_repo =
            Repository::open(&container_path).expect("Failed to open container as repo");
        assert!(container_repo.is_bare(), "Container repo should be bare");

        let wt1_path = container_path.join("worktree1");
        add_worktree_to_bare(&container_repo, "worktree1", &wt1_path);
        let wt2_path = container_path.join("worktree2");
        add_worktree_to_bare(&container_repo, "worktree2", &wt2_path);

        assert!(
            is_bare_repo_worktree_exclusive_container(&container_path, &container_repo),
            "Should be an exclusive bare repo container"
        );
    }

    #[test]
    fn test_is_bare_repo_worktree_container_true_despite_extra_file_relaxed() {
        let base_dir = tempdir().unwrap();
        let container_path = base_dir.path().join("project_container_extra_file");
        fs::create_dir(&container_path).unwrap();

        let bare_repo_actual_path = container_path.join("actual_bare.git");
        fs::create_dir(&bare_repo_actual_path).unwrap();
        let _ = init_bare_repo(&bare_repo_actual_path);
        fs::write(
            container_path.join(".git"),
            format!(
                "gitdir: {}",
                bare_repo_actual_path
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
            ),
        )
        .unwrap();

        let container_repo = Repository::open(&container_path).unwrap();

        let wt1_path = container_path.join("worktree1");
        add_worktree_to_bare(&container_repo, "worktree1", &wt1_path);

        File::create(container_path.join("extra_file.txt")).unwrap();

        assert!(
            is_bare_repo_worktree_exclusive_container(&container_path, &container_repo),
            "Should be a container despite extra file (relaxed check)"
        );
    }

    #[test]
    fn test_is_bare_repo_worktree_container_true_despite_unrelated_dir_relaxed() {
        let base_dir = tempdir().unwrap();
        let container_path = base_dir.path().join("project_container_extra_dir");
        fs::create_dir(&container_path).unwrap();

        let bare_repo_actual_path = container_path.join("actual_bare.git");
        fs::create_dir(&bare_repo_actual_path).unwrap();
        let _ = init_bare_repo(&bare_repo_actual_path);
        fs::write(
            container_path.join(".git"),
            format!(
                "gitdir: {}",
                bare_repo_actual_path
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
            ),
        )
        .unwrap();

        let container_repo = Repository::open(&container_path).unwrap();

        let wt1_path = container_path.join("worktree1");
        add_worktree_to_bare(&container_repo, "worktree1", &wt1_path);

        fs::create_dir(container_path.join("unrelated_dir")).unwrap();

        assert!(
            is_bare_repo_worktree_exclusive_container(&container_path, &container_repo),
            "Should be a container despite unrelated dir (relaxed check)"
        );
    }

    #[test]
    fn test_is_bare_repo_worktree_exclusive_container_false_no_worktrees() {
        let base_dir = tempdir().unwrap();
        let container_path = base_dir.path().join("project_container_no_wt");
        fs::create_dir(&container_path).unwrap();

        let bare_repo_actual_path = container_path.join("actual_bare.git");
        fs::create_dir(&bare_repo_actual_path).unwrap();
        let _ = init_bare_repo(&bare_repo_actual_path);
        fs::write(
            container_path.join(".git"),
            format!(
                "gitdir: {}",
                bare_repo_actual_path
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
            ),
        )
        .unwrap();

        let container_repo = Repository::open(&container_path).unwrap();

        assert!(
            !is_bare_repo_worktree_exclusive_container(&container_path, &container_repo),
            "Should not be an exclusive container as there are no worktrees"
        );
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

        assert!(check_if_worktree_container(container_dir.path()));
    }

    #[test]
    fn test_check_if_worktree_container_one_worktree() {
        let main_repo_dir = tempdir().unwrap();
        let _main_repo = init_bare_repo(main_repo_dir.path());

        let container_dir = tempdir().unwrap();
        let wt1_path = container_dir.path().join("wt1");
        add_worktree_to_bare(&_main_repo, "wt1", &wt1_path);

        assert!(check_if_worktree_container(container_dir.path()));
    }

    #[test]
    fn test_check_if_worktree_container_empty_dir() {
        let container_dir = tempdir().unwrap();
        assert!(!check_if_worktree_container(container_dir.path()));
    }

    #[test]
    fn test_check_if_worktree_container_with_file() {
        let main_repo_dir = tempdir().unwrap();
        let _main_repo = init_bare_repo(main_repo_dir.path());

        let container_dir = tempdir().unwrap();
        let wt1_path = container_dir.path().join("wt1");
        add_worktree_to_bare(&_main_repo, "wt1", &wt1_path);
        File::create(container_dir.path().join("some_file.txt")).unwrap();

        assert!(!check_if_worktree_container(container_dir.path()));
    }

    #[test]
    fn test_check_if_worktree_container_with_plain_dir() {
        let main_repo_dir = tempdir().unwrap();
        let _main_repo = init_bare_repo(main_repo_dir.path());

        let container_dir = tempdir().unwrap();
        let wt1_path = container_dir.path().join("wt1");
        add_worktree_to_bare(&_main_repo, "wt1", &wt1_path);
        fs::create_dir(container_dir.path().join("plain_dir")).unwrap();

        assert!(!check_if_worktree_container(container_dir.path()));
    }

    #[test]
    fn test_check_if_worktree_container_with_non_worktree_repo() {
        let main_repo_dir = tempdir().unwrap();
        let _main_repo = init_bare_repo(main_repo_dir.path());

        let container_dir = tempdir().unwrap();
        let wt1_path = container_dir.path().join("wt1");
        add_worktree_to_bare(&_main_repo, "wt1", &wt1_path);
        // Helper to init standard repo (not bare)
        Repository::init(container_dir.path().join("other_repo"))
            .expect("Failed to init other_repo");

        assert!(!check_if_worktree_container(container_dir.path()));
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

        assert!(!check_if_worktree_container(container_dir.path()));
    }

    #[test]
    #[cfg(unix)]
    fn test_check_if_worktree_container_with_symlink_to_worktree() {
        let main_repo_temp_dir = tempdir().unwrap();
        let _main_repo = init_bare_repo(main_repo_temp_dir.path());

        let actual_wt_parent_dir = tempdir().unwrap();
        let actual_wt_physical_path = actual_wt_parent_dir.path().join("actual_wt1_loc");
        add_worktree_to_bare(&_main_repo, "actual_wt1", &actual_wt_physical_path);

        let container_dir = tempdir().unwrap();
        let symlink_path = container_dir.path().join("sym_wt1");
        std::os::unix::fs::symlink(&actual_wt_physical_path, symlink_path).unwrap();

        assert!(check_if_worktree_container(container_dir.path()));
    }

    #[test]
    #[cfg(unix)]
    fn test_check_if_worktree_container_with_symlink_to_file() {
        let main_repo_temp_dir = tempdir().unwrap();
        let _main_repo = init_bare_repo(main_repo_temp_dir.path());

        let container_dir = tempdir().unwrap();
        let wt1_path = container_dir.path().join("wt1");
        add_worktree_to_bare(&_main_repo, "wt1", &wt1_path);

        let file_target_temp_dir = tempdir().unwrap();
        let file_path = file_target_temp_dir.path().join("target_file.txt");
        File::create(&file_path).unwrap();
        let symlink_path = container_dir.path().join("sym_to_file");
        std::os::unix::fs::symlink(&file_path, symlink_path).unwrap();

        assert!(!check_if_worktree_container(container_dir.path()));
    }
}
