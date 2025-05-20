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
    let reference = bare_repo
        .find_reference(&format!("refs/heads/{branch_name}"))
        .unwrap();
    opts.reference(Some(&reference));
    bare_repo
        .worktree(worktree_name, worktree_path, Some(&opts))
        .expect("Failed to add worktree to bare repo")
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
    bare_repo
        .branch(branch_name, &initial_commit, false)
        .unwrap();

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
            None,
            &signature,
            &signature,
            "Initial commit for bare repo",
            &tree,
            &[],
        )
        .expect("Failed to create initial commit in bare repo");
    let initial_commit = bare_repo.find_commit(initial_commit_oid).unwrap();

    // Create a branch pointing to the initial commit
    let branch_name = "main";
    bare_repo
        .branch(branch_name, &initial_commit, false)
        .unwrap();

    // Add a worktree
    let base_wt_temp_dir = tempdir().unwrap();
    let wt_path = base_wt_temp_dir
        .path()
        .join("bare_worktree_for_main_path_test");
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
