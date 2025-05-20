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
        let mut index = bare_repo
            .index()
            .expect("Failed to get index for bare repo");
        let tree_id = index.write_tree().expect("Failed to write empty tree");
        let tree = bare_repo.find_tree(tree_id).expect("Failed to find tree");
        let sig =
            Signature::now("Test User", "test@example.com").expect("Failed to create signature");
        bare_repo
            .commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
            .expect("Failed to create initial commit in bare repo");
    }
    fs::create_dir_all(worktree_path.parent().unwrap())
        .expect("Failed to create parent for worktree path");
    let opts = WorktreeAddOptions::new();
    bare_repo
        .worktree(worktree_name, worktree_path, Some(&opts))
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
            bare_repo_actual_path.file_name().unwrap().to_str().unwrap()
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
        is_bare_repo_worktree_exclusive_container(&container_path, &container_repo).unwrap(),
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
            bare_repo_actual_path.file_name().unwrap().to_str().unwrap()
        ),
    )
    .unwrap();

    let container_repo = Repository::open(&container_path).unwrap();

    let wt1_path = container_path.join("worktree1");
    add_worktree_to_bare(&container_repo, "worktree1", &wt1_path);

    File::create(container_path.join("extra_file.txt")).unwrap();

    assert!(
        is_bare_repo_worktree_exclusive_container(&container_path, &container_repo).unwrap(),
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
            bare_repo_actual_path.file_name().unwrap().to_str().unwrap()
        ),
    )
    .unwrap();

    let container_repo = Repository::open(&container_path).unwrap();

    let wt1_path = container_path.join("worktree1");
    add_worktree_to_bare(&container_repo, "worktree1", &wt1_path);

    fs::create_dir(container_path.join("unrelated_dir")).unwrap();

    assert!(
        is_bare_repo_worktree_exclusive_container(&container_path, &container_repo).unwrap(),
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
            bare_repo_actual_path.file_name().unwrap().to_str().unwrap()
        ),
    )
    .unwrap();

    let container_repo = Repository::open(&container_path).unwrap();

    assert!(
        !is_bare_repo_worktree_exclusive_container(&container_path, &container_repo).unwrap(),
        "Should not be an exclusive container as there are no worktrees"
    );
}

#[test]
fn test_check_if_worktree_container_valid_two_worktrees() {
    let main_repo_dir = tempdir().unwrap();
    let main_repo = init_bare_repo(main_repo_dir.path());

    let container_dir = tempdir().unwrap();
    let wt1_path = container_dir.path().join("wt1");
    let wt2_path = container_dir.path().join("wt2");
    add_worktree_to_bare(&main_repo, "wt1", &wt1_path);
    add_worktree_to_bare(&main_repo, "wt2", &wt2_path);

    assert!(check_if_worktree_container(container_dir.path()).unwrap());
}

#[test]
fn test_check_if_worktree_container_one_worktree() {
    let main_repo_dir = tempdir().unwrap();
    let main_repo = init_bare_repo(main_repo_dir.path());

    let container_dir = tempdir().unwrap();
    let wt1_path = container_dir.path().join("wt1");
    add_worktree_to_bare(&main_repo, "wt1", &wt1_path);

    assert!(check_if_worktree_container(container_dir.path()).unwrap());
}

#[test]
fn test_check_if_worktree_container_empty_dir() {
    let container_dir = tempdir().unwrap();
    assert!(!check_if_worktree_container(container_dir.path()).unwrap());
}

#[test]
fn test_check_if_worktree_container_with_file() {
    let main_repo_dir = tempdir().unwrap();
    let main_repo = init_bare_repo(main_repo_dir.path());

    let container_dir = tempdir().unwrap();
    let wt1_path = container_dir.path().join("wt1");
    add_worktree_to_bare(&main_repo, "wt1", &wt1_path);
    File::create(container_dir.path().join("some_file.txt")).unwrap();

    assert!(!check_if_worktree_container(container_dir.path()).unwrap());
}

#[test]
fn test_check_if_worktree_container_with_plain_dir() {
    let main_repo_dir = tempdir().unwrap();
    let main_repo = init_bare_repo(main_repo_dir.path());

    let container_dir = tempdir().unwrap();
    let wt1_path = container_dir.path().join("wt1");
    add_worktree_to_bare(&main_repo, "wt1", &wt1_path);
    fs::create_dir(container_dir.path().join("plain_dir")).unwrap();

    assert!(!check_if_worktree_container(container_dir.path()).unwrap());
}

#[test]
fn test_check_if_worktree_container_with_non_worktree_repo() {
    let main_repo_dir = tempdir().unwrap();
    let main_repo = init_bare_repo(main_repo_dir.path());

    let container_dir = tempdir().unwrap();
    let wt1_path = container_dir.path().join("wt1");
    add_worktree_to_bare(&main_repo, "wt1", &wt1_path);
    // Helper to init standard repo (not bare)
    Repository::init(container_dir.path().join("other_repo")).expect("Failed to init other_repo");

    assert!(!check_if_worktree_container(container_dir.path()).unwrap());
}

#[test]
fn test_check_if_worktree_container_different_main_repos() {
    let dir_main_repo_a = tempdir().unwrap();
    let main_repo_a = init_bare_repo(dir_main_repo_a.path());
    let dir_main_repo_b = tempdir().unwrap();
    let main_repo_b = init_bare_repo(dir_main_repo_b.path());

    let container_dir = tempdir().unwrap();
    let wt_a_path = container_dir.path().join("wt_a");
    let wt_b_path = container_dir.path().join("wt_b");
    add_worktree_to_bare(&main_repo_a, "wt_a", &wt_a_path);
    add_worktree_to_bare(&main_repo_b, "wt_b", &wt_b_path);

    assert!(!check_if_worktree_container(container_dir.path()).unwrap());
}

#[test]
#[cfg(unix)]
fn test_check_if_worktree_container_with_symlink_to_worktree() {
    let main_repo_temp_dir = tempdir().unwrap();
    let main_repo = init_bare_repo(main_repo_temp_dir.path());

    let actual_wt_parent_dir = tempdir().unwrap();
    let actual_wt_physical_path = actual_wt_parent_dir.path().join("actual_wt1_loc");
    add_worktree_to_bare(&main_repo, "actual_wt1", &actual_wt_physical_path);

    let container_dir = tempdir().unwrap();
    let symlink_path = container_dir.path().join("sym_wt1");
    std::os::unix::fs::symlink(&actual_wt_physical_path, symlink_path).unwrap();

    assert!(check_if_worktree_container(container_dir.path()).unwrap());
}

#[test]
#[cfg(unix)]
fn test_check_if_worktree_container_with_symlink_to_file() {
    let main_repo_temp_dir = tempdir().unwrap();
    let main_repo = init_bare_repo(main_repo_temp_dir.path());

    let container_dir = tempdir().unwrap();
    let wt1_path = container_dir.path().join("wt1");
    add_worktree_to_bare(&main_repo, "wt1", &wt1_path);

    let file_target_temp_dir = tempdir().unwrap();
    let file_path = file_target_temp_dir.path().join("target_file.txt");
    File::create(&file_path).unwrap();
    let symlink_path = container_dir.path().join("sym_to_file");
    std::os::unix::fs::symlink(&file_path, symlink_path).unwrap();

    assert!(!check_if_worktree_container(container_dir.path()).unwrap());
}
