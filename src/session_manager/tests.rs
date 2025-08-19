use super::*;
use crate::directory_scanner::{DirectoryEntry, DirectoryType};
use std::path::PathBuf;

#[test]
fn test_generate_session_name_simple() {
    let path = PathBuf::from("/path/to/my.project");
    let name = SessionManager::generate_session_name(&path, None);
    assert_eq!(name, "my-project");
}

#[test]
fn test_generate_session_name_with_colon() {
    let path = PathBuf::from("/path/to/project:name");
    let name = SessionManager::generate_session_name(&path, None);
    assert_eq!(name, "project-name");
}

#[test]
fn test_generate_session_name_worktree() {
    let item_path = PathBuf::from("/path/to/main_repo/worktrees/feature.branch");
    let parent_repo_path = PathBuf::from("/path/to/main_repo");
    let name = SessionManager::generate_session_name(&item_path, Some(&parent_repo_path));
    assert_eq!(name, "main_repo_feature-branch");
}

#[test]
fn test_generate_session_name_worktree_with_dots_in_parent() {
    let item_path = PathBuf::from("/path/to/parent.repo/worktrees/my_feature");
    let parent_repo_path = PathBuf::from("/path/to/parent.repo");
    let name = SessionManager::generate_session_name(&item_path, Some(&parent_repo_path));
    assert_eq!(name, "parent-repo_my_feature");
}

#[test]
fn test_generate_session_name_root_path_item() {
    let item_path = PathBuf::from("/");
    let name = SessionManager::generate_session_name(&item_path, None);
    assert_eq!(name, "default_session");
}

#[test]
fn test_generate_session_name_root_path_parent() {
    let item_path = PathBuf::from("/some/project");
    let parent_repo_path = PathBuf::from("/");
    let name = SessionManager::generate_session_name(&item_path, Some(&parent_repo_path));
    assert_eq!(name, "default_parent_project");
}

#[test]
fn test_create_selection_from_directory_entry_plain() {
    let entry = DirectoryEntry {
        // Assuming DirectoryEntry has these fields based on previous context
        path: PathBuf::from("/path/to/my.project"), // Use 'path' instead of 'original_path'
        resolved_path: PathBuf::from("/path/to/my.project"),
        display_name: "my.project".to_string(),
        entry_type: DirectoryType::Plain,
        parent_path: None,
    };

    let selection = SessionManager::create_selection_from_directory_entry(&entry);

    assert_eq!(selection.path, PathBuf::from("/path/to/my.project"));
    assert_eq!(selection.display_name, "my.project");
    // Uses generate_session_name logic tested elsewhere
    assert_eq!(selection.session_name, "my-project");
}

#[test]
fn test_create_selection_from_directory_entry_worktree() {
    let main_repo_path = PathBuf::from("/path/to/parent.repo");
    let worktree_path = main_repo_path.join("worktrees").join("feature-branch");

    let entry = DirectoryEntry {
        path: worktree_path.clone(), // Use 'path' instead of 'original_path'
        resolved_path: worktree_path.clone(),
        display_name: "feature-branch (parent.repo)".to_string(), // Example display name
        entry_type: DirectoryType::GitWorktree {
            main_worktree_path: main_repo_path.clone(),
        },
        parent_path: Some(main_repo_path.clone()),
    };

    let selection = SessionManager::create_selection_from_directory_entry(&entry);

    assert_eq!(selection.path, worktree_path);
    assert_eq!(selection.display_name, "feature-branch (parent.repo)");
    // Uses generate_session_name logic for worktrees tested elsewhere
    assert_eq!(selection.session_name, "parent-repo_feature-branch");
}

#[test]
fn test_create_new_project_directory_success() {
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let parent_path = temp_dir.path();
    let project_name = "test-project";

    let selection = SessionManager::create_new_project_directory(project_name, parent_path).unwrap();

    // Verify the directory was created
    let project_path = parent_path.join(project_name);
    assert!(project_path.exists());
    assert!(project_path.is_dir());

    // Verify the selection structure
    assert_eq!(selection.path, project_path);
    assert_eq!(selection.display_name, project_name);
    assert_eq!(selection.session_name, "test-project");
}

#[test]
fn test_create_new_project_directory_already_exists() {
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let parent_path = temp_dir.path();
    let project_name = "existing-project";
    let project_path = parent_path.join(project_name);

    // Create the directory first
    std::fs::create_dir(&project_path).unwrap();

    // Attempt to create it again should fail
    let result = SessionManager::create_new_project_directory(project_name, parent_path);
    assert!(result.is_err());
    
    match result.unwrap_err() {
        AppError::Session(msg) => {
            assert!(msg.contains("already exists"));
        }
        _ => panic!("Expected Session error"),
    }
}

#[test]
fn test_create_new_project_directory_creates_parent() {
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let parent_path = temp_dir.path().join("nested/deep/path");
    let project_name = "test-project";

    // Parent path doesn't exist
    assert!(!parent_path.exists());

    let selection = SessionManager::create_new_project_directory(project_name, &parent_path).unwrap();

    // Verify both parent and project directories were created
    assert!(parent_path.exists());
    let project_path = parent_path.join(project_name);
    assert!(project_path.exists());
    assert!(project_path.is_dir());

    // Verify the selection structure
    assert_eq!(selection.path, project_path);
    assert_eq!(selection.display_name, project_name);
    assert_eq!(selection.session_name, "test-project");
}

// Keep the existing note about tests requiring tmux interaction
// Note: Tests for `is_tmux_server_running`, `session_exists`, `create_new_session`,
// and `switch_or_attach_to_session` would require a live tmux server
// or mocking the `tmux_interface` calls, which is complex for unit tests.
// These functions are better suited for integration testing.
