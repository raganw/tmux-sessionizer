use super::*;
use crate::directory_scanner::DirectoryType;
use tempfile::tempdir;

#[test]
fn test_format_directory_entry_for_skim_plain() {
    let entry = DirectoryEntry {
        path: PathBuf::from("/original/path/to/project_a"),
        resolved_path: PathBuf::from("/resolved/path/to/project_a"),
        display_name: "project_a".to_string(),
        entry_type: DirectoryType::Plain,
        parent_path: None,
    };
    assert_eq!(
        FuzzyFinder::format_directory_entry_for_skim(&entry),
        "project_a\t/resolved/path/to/project_a"
    );
}

#[test]
fn test_format_directory_entry_for_skim_git_repo() {
    let entry = DirectoryEntry {
        path: PathBuf::from("/original/git_repo"),
        resolved_path: PathBuf::from("/resolved/git_repo"),
        display_name: "git_repo".to_string(),
        entry_type: DirectoryType::GitRepository,
        parent_path: None,
    };
    assert_eq!(
        FuzzyFinder::format_directory_entry_for_skim(&entry),
        "git_repo\t/resolved/git_repo"
    );
}

#[test]
fn test_format_directory_entry_for_skim_worktree() {
    let main_repo_path = PathBuf::from("/resolved/main_repo");
    let entry = DirectoryEntry {
        path: PathBuf::from("/original/main_repo/worktree_x"),
        resolved_path: PathBuf::from("/resolved/main_repo/worktree_x"),
        display_name: "[main_repo] worktree_x".to_string(),
        entry_type: DirectoryType::GitWorktree {
            main_worktree_path: main_repo_path.clone(),
        },
        parent_path: Some(main_repo_path),
    };
    assert_eq!(
        FuzzyFinder::format_directory_entry_for_skim(&entry),
        "[main_repo] worktree_x\t/resolved/main_repo/worktree_x"
    );
}

#[test]
fn test_prepare_skim_input_empty() {
    let entries = Vec::new();
    assert_eq!(FuzzyFinder::prepare_skim_input(&entries), "");
}

#[test]
fn test_prepare_skim_input_multiple_entries() {
    let entries = vec![
        DirectoryEntry {
            path: PathBuf::from("/orig/p1"),
            resolved_path: PathBuf::from("/res/p1"),
            display_name: "p1".to_string(),
            entry_type: DirectoryType::Plain,
            parent_path: None,
        },
        DirectoryEntry {
            path: PathBuf::from("/orig/p2"),
            resolved_path: PathBuf::from("/res/p2"),
            display_name: "p2_display".to_string(),
            entry_type: DirectoryType::GitRepository,
            parent_path: None,
        },
    ];
    let expected_output = "p1\t/res/p1\np2_display\t/res/p2";
    assert_eq!(FuzzyFinder::prepare_skim_input(&entries), expected_output);
}

#[test]
fn test_select_with_empty_entries_returns_ok_none() {
    let entries = Vec::new();
    let result = FuzzyFinder::select(&entries);
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

// Helper to create DirectoryEntry for direct_select tests
fn new_test_entry(p_str: &str, rp_str: &str, dn_str: &str) -> DirectoryEntry {
    DirectoryEntry {
        path: PathBuf::from(p_str),
        resolved_path: PathBuf::from(rp_str),
        display_name: dn_str.to_string(),
        entry_type: DirectoryType::Plain,
        parent_path: None,
    }
}

#[test]
fn test_direct_select_empty_entries() {
    let entries = Vec::new();
    let result = FuzzyFinder::direct_select(&entries, "anything");
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[test]
fn test_direct_select_no_match() {
    let entries = vec![new_test_entry(
        "/path/to/project_a",
        "/resolved/project_a",
        "project_a",
    )];
    let result = FuzzyFinder::direct_select(&entries, "nonexistent_project");
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[test]
fn test_direct_select_canonical_path_match() {
    let temp_dir = tempdir().unwrap();
    let project_path = temp_dir.path().join("my_project");
    fs::create_dir(&project_path).unwrap();
    let canonical_project_path = fs::canonicalize(&project_path).unwrap();

    let entries = vec![DirectoryEntry {
        path: project_path.clone(),
        resolved_path: canonical_project_path.clone(),
        display_name: "my_project_display".to_string(),
        entry_type: DirectoryType::Plain,
        parent_path: None,
    }];
    let result = FuzzyFinder::direct_select(&entries, project_path.to_str().unwrap());
    assert!(result.is_ok());
    let selection = result.unwrap().expect("Should have found a selection");
    assert_eq!(selection.display_name, "my_project_display");
    assert_eq!(selection.path, canonical_project_path);
    fs::remove_dir(&project_path).unwrap();
}

#[test]
fn test_direct_select_original_path_match() {
    let temp_target_dir = tempdir().unwrap();
    let project_target_path = temp_target_dir.path().join("actual_project");
    fs::create_dir(&project_target_path).unwrap();
    let canonical_project_target_path = fs::canonicalize(&project_target_path).unwrap();

    let temp_link_dir = tempdir().unwrap();
    let symlink_path = temp_link_dir.path().join("project_link");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&project_target_path, &symlink_path).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&project_target_path, &symlink_path).unwrap();

    // Skip test if symlink creation failed (e.g. permissions on Windows, or not supported)
    if !symlink_path.exists() && !symlink_path.is_symlink() {
        tracing::warn!(
            "Symlink creation failed or not supported, skipping test_direct_select_original_path_match"
        );
        return;
    }

    let entries = vec![DirectoryEntry {
        path: symlink_path.clone(),
        resolved_path: canonical_project_target_path.clone(),
        display_name: "linked_project".to_string(),
        entry_type: DirectoryType::Plain,
        parent_path: None,
    }];
    let result = FuzzyFinder::direct_select(&entries, symlink_path.to_str().unwrap());
    assert!(result.is_ok(), "Result was: {:?}", result.err());
    let selection = result
        .unwrap()
        .expect("Should have found a selection by original path");
    assert_eq!(selection.display_name, "linked_project");
    assert_eq!(selection.path, canonical_project_target_path);

    // fs::remove_file on symlink, or remove_dir if it's a directory symlink
    #[cfg(unix)]
    fs::remove_file(&symlink_path)
        .unwrap_or_else(|e| tracing::warn!("Failed to remove symlink file: {}", e));
    #[cfg(windows)]
    fs::remove_dir(&symlink_path)
        .unwrap_or_else(|e| tracing::warn!("Failed to remove symlink dir: {}", e));
    fs::remove_dir(&project_target_path).unwrap();
}

#[test]
fn test_direct_select_suffix_match_unique() {
    let entries = vec![
        new_test_entry(
            "/p/to/project_a",
            "/resolved/path/to/project_a",
            "project_a",
        ),
        new_test_entry(
            "/a/p/project_b",
            "/resolved/another/path/project_b",
            "project_b",
        ),
    ];
    let result = FuzzyFinder::direct_select(&entries, "to/project_a");
    assert!(result.is_ok());
    assert_eq!(result.unwrap().unwrap().display_name, "project_a");
}

#[test]
fn test_direct_select_display_name_match_unique() {
    let entries = vec![
        new_test_entry("/p/proj1", "/resolved/proj1", "unique_name_1"),
        new_test_entry("/p/proj2", "/resolved/proj2", "unique_name_2"),
    ];
    let result = FuzzyFinder::direct_select(&entries, "unique_name_1");
    assert!(result.is_ok());
    assert_eq!(
        result.unwrap().unwrap().path,
        PathBuf::from("/resolved/proj1")
    );
}

#[test]
fn test_direct_select_filename_match_ambiguous() {
    let entries = vec![
        new_test_entry(
            "/some/common_name",
            "/resolved/some/path/common_name",
            "display1",
        ),
        new_test_entry(
            "/another/common_name",
            "/resolved/another/path/common_name",
            "display2",
        ),
    ];
    let result = FuzzyFinder::direct_select(&entries, "common_name");
    assert!(result.is_err());
}

#[test]
fn test_new_project_request_creation() {
    use crate::fuzzy_finder_interface::{NewProjectRequest, SelectionResult};
    use std::path::PathBuf;

    let request = NewProjectRequest {
        project_name: "test-project".to_string(),
        parent_path: PathBuf::from("/home/user/projects"),
    };

    let result = SelectionResult::NewProject(request.clone());
    
    match result {
        SelectionResult::NewProject(req) => {
            assert_eq!(req.project_name, "test-project");
            assert_eq!(req.parent_path, PathBuf::from("/home/user/projects"));
        }
        _ => panic!("Expected NewProject variant"),
    }
}

#[test] 
fn test_selection_result_existing_project() {
    use crate::fuzzy_finder_interface::{SelectedItem, SelectionResult};
    use std::path::PathBuf;

    let item = SelectedItem {
        display_name: "existing-project".to_string(),
        path: PathBuf::from("/path/to/existing"),
    };

    let result = SelectionResult::ExistingProject(item.clone());
    
    match result {
        SelectionResult::ExistingProject(selected) => {
            assert_eq!(selected.display_name, "existing-project");
            assert_eq!(selected.path, PathBuf::from("/path/to/existing"));
        }
        _ => panic!("Expected ExistingProject variant"),
    }
}
