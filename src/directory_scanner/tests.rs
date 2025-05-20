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
        let sig =
            Signature::now("Test User", "test@example.com").expect("Failed to create signature");
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
        let sig = Signature::now("test", "test@example.com").expect("Failed to create signature");
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
        ..Default::default()
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

    let entry_wt_a = entries
        .iter()
        .find(|e| e.resolved_path.ends_with("wt_a"))
        .expect("wt_a not found");
    assert_eq!(entry_wt_a.display_name, "[bare_repo.git] wt_a");
    match &entry_wt_a.entry_type {
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
            entry_wt_a.entry_type
        ),
    }

    let entry_wt_b = entries
        .iter()
        .find(|e| e.resolved_path.ends_with("wt_b"))
        .expect("wt_b not found");
    assert_eq!(entry_wt_b.display_name, "[bare_repo.git] wt_b");
    match &entry_wt_b.entry_type {
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
            entry_wt_b.entry_type
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
    let count_wt_a = entries
        .iter()
        .filter(|e| e.resolved_path.ends_with("wt_a"))
        .count();
    assert_eq!(count_wt_a, 1, "wt_a should appear exactly once");
    let count_wt_b = entries
        .iter()
        .filter(|e| e.resolved_path.ends_with("wt_b"))
        .count();
    assert_eq!(count_wt_b, 1, "wt_b should appear exactly once");

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
