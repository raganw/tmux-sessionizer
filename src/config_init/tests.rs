use super::*;
use std::env;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_config_initializer_new_creates_paths() {
    let temp_dir = tempdir().unwrap();
    let original_xdg_config_home = env::var_os("XDG_CONFIG_HOME");

    // Set XDG_CONFIG_HOME to our temp directory
    unsafe {
        env::set_var("XDG_CONFIG_HOME", temp_dir.path());
    }

    let initializer = ConfigInitializer::new().unwrap();

    // Restore original XDG_CONFIG_HOME
    if let Some(val) = original_xdg_config_home {
        unsafe {
            env::set_var("XDG_CONFIG_HOME", val);
        }
    } else {
        unsafe {
            env::remove_var("XDG_CONFIG_HOME");
        }
    }

    // Check that paths are set correctly
    assert!(initializer.config_dir.ends_with("tmux-sessionizer"));
    assert!(initializer.config_file.ends_with("tmux-sessionizer.toml"));
    assert!(initializer.config_dir.is_absolute());
    assert!(initializer.config_file.is_absolute());
}

#[test]
fn test_create_config_directory_success() {
    let temp_dir = tempdir().unwrap();
    let config_dir = temp_dir.path().join("tmux-sessionizer");
    let config_file = config_dir.join("tmux-sessionizer.toml");

    let initializer = ConfigInitializer {
        config_dir: config_dir.clone(),
        config_file,
    };

    // Directory should not exist yet
    assert!(!config_dir.exists());

    // Create directory
    initializer.create_config_directory().unwrap();

    // Directory should now exist
    assert!(config_dir.exists());
    assert!(config_dir.is_dir());
}

#[test]
fn test_create_config_directory_already_exists() {
    let temp_dir = tempdir().unwrap();
    let config_dir = temp_dir.path().join("tmux-sessionizer");
    let config_file = config_dir.join("tmux-sessionizer.toml");

    // Create directory beforehand
    fs::create_dir_all(&config_dir).unwrap();

    let initializer = ConfigInitializer {
        config_dir: config_dir.clone(),
        config_file,
    };

    // Should succeed even if directory already exists
    initializer.create_config_directory().unwrap();

    // Directory should still exist
    assert!(config_dir.exists());
    assert!(config_dir.is_dir());
}

#[test]
fn test_create_template_file_success() {
    let temp_dir = tempdir().unwrap();
    let config_dir = temp_dir.path().join("tmux-sessionizer");
    let config_file = config_dir.join("tmux-sessionizer.toml");

    // Create the directory first
    fs::create_dir_all(&config_dir).unwrap();

    let initializer = ConfigInitializer {
        config_dir,
        config_file: config_file.clone(),
    };

    // File should not exist yet
    assert!(!config_file.exists());

    // Create template file
    initializer.create_template_file().unwrap();

    // File should now exist
    assert!(config_file.exists());
    assert!(config_file.is_file());

    // Check content contains expected elements
    let content = fs::read_to_string(&config_file).unwrap();
    assert!(content.contains("Example configuration file for tmux-sessionizer"));
    assert!(content.contains("# search_paths = ["));
    assert!(content.contains("# additional_paths = ["));
    assert!(content.contains("# exclude_patterns = ["));
}

#[test]
fn test_create_template_file_already_exists() {
    let temp_dir = tempdir().unwrap();
    let config_dir = temp_dir.path().join("tmux-sessionizer");
    let config_file = config_dir.join("tmux-sessionizer.toml");

    // Create the directory first
    fs::create_dir_all(&config_dir).unwrap();

    // Create file with existing content
    fs::write(&config_file, "existing content").unwrap();

    let initializer = ConfigInitializer {
        config_dir,
        config_file: config_file.clone(),
    };

    // Should succeed and not overwrite
    initializer.create_template_file().unwrap();

    // File should still exist with original content
    assert!(config_file.exists());
    let content = fs::read_to_string(&config_file).unwrap();
    assert_eq!(content, "existing content");
}

#[test]
fn test_validate_created_file_success() {
    let temp_dir = tempdir().unwrap();
    let config_dir = temp_dir.path().join("tmux-sessionizer");
    let config_file = config_dir.join("tmux-sessionizer.toml");

    // Create the directory and file
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(&config_file, "test content").unwrap();

    let initializer = ConfigInitializer {
        config_dir,
        config_file,
    };

    // Validation should succeed
    initializer.validate_created_file().unwrap();
}

#[test]
fn test_validate_created_file_not_found() {
    let temp_dir = tempdir().unwrap();
    let config_dir = temp_dir.path().join("tmux-sessionizer");
    let config_file = config_dir.join("tmux-sessionizer.toml");

    let initializer = ConfigInitializer {
        config_dir,
        config_file,
    };

    // Validation should fail because file doesn't exist
    let result = initializer.validate_created_file();
    assert!(result.is_err());

    match result.unwrap_err() {
        ConfigError::ValidationFailed { path, .. } => {
            assert!(path.ends_with("tmux-sessionizer.toml"));
        }
        other => panic!("Expected ValidationFailed error, got {:?}", other),
    }
}

#[test]
fn test_generate_template_content() {
    let content = ConfigInitializer::generate_template_content();

    // Check that all configuration sections are commented out
    assert!(content.contains("# search_paths = ["));
    assert!(content.contains("# additional_paths = ["));
    assert!(content.contains("# exclude_patterns = ["));

    // Check that it contains explanatory text
    assert!(content.contains("Example configuration file for tmux-sessionizer"));
    assert!(content.contains("--- Search Paths ---"));
    assert!(content.contains("--- Additional Paths ---"));
    assert!(content.contains("--- Exclusion Patterns ---"));

    // Ensure no uncommented configuration lines
    let lines: Vec<&str> = content.lines().collect();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with("search_paths")
            || trimmed.starts_with("additional_paths")
            || trimmed.starts_with("exclude_patterns")
        {
            panic!("Found uncommented configuration line: {}", line);
        }
    }
}

#[test]
fn test_init_config_full_workflow() {
    // Use a more specific temp directory to avoid conflicts
    let temp_dir = tempdir().unwrap();
    let unique_path = temp_dir
        .path()
        .join(format!("config_{}", std::process::id()));
    let original_xdg_config_home = env::var_os("XDG_CONFIG_HOME");

    // Set XDG_CONFIG_HOME to our unique temp directory
    unsafe {
        env::set_var("XDG_CONFIG_HOME", &unique_path);
    }

    let initializer = ConfigInitializer::new().unwrap();

    // Restore original XDG_CONFIG_HOME
    if let Some(val) = original_xdg_config_home {
        unsafe {
            env::set_var("XDG_CONFIG_HOME", val);
        }
    } else {
        unsafe {
            env::remove_var("XDG_CONFIG_HOME");
        }
    }

    // Neither directory nor file should exist yet
    assert!(!initializer.config_dir.exists());
    assert!(!initializer.config_file.exists());

    // Run full initialization
    let file_was_created = initializer.init_config().unwrap();
    assert!(file_was_created); // Should be true since file didn't exist before

    // Both directory and file should now exist
    assert!(initializer.config_dir.exists());
    assert!(initializer.config_dir.is_dir());
    assert!(initializer.config_file.exists());
    assert!(initializer.config_file.is_file());

    // Check file content
    let content = fs::read_to_string(&initializer.config_file).unwrap();
    assert!(content.contains("Example configuration file for tmux-sessionizer"));
}

#[test]
fn test_init_config_when_file_already_exists() {
    let temp_dir = tempdir().unwrap();
    let config_dir = temp_dir.path().join("tmux-sessionizer");
    let config_file = config_dir.join("tmux-sessionizer.toml");
    
    // Create the directory and file beforehand
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(&config_file, "existing config content").unwrap();
    
    let initializer = ConfigInitializer {
        config_dir: config_dir.clone(),
        config_file: config_file.clone(),
    };
    
    // Run initialization - should not overwrite existing file
    let file_was_created = initializer.init_config().unwrap();
    assert!(!file_was_created); // Should be false since file already existed
    
    // File should still exist with original content
    assert!(config_file.exists());
    let content = fs::read_to_string(&config_file).unwrap();
    assert_eq!(content, "existing config content");
}

// Error scenario tests

#[test]
fn test_create_config_directory_permission_denied() {
    // This test is challenging to implement portably because permission errors
    // depend on the operating system and user permissions.
    // For now, we rely on the error handling code paths being correct.
    // A more robust test would require creating a directory with restricted permissions
    // and attempting to create a subdirectory within it.

    // We'll test the error type structure instead
    use std::io;
    let fake_path = PathBuf::from("/non/existent/path/that/should/fail");
    let fake_config_file = fake_path.join("tmux-sessionizer.toml");

    let initializer = ConfigInitializer {
        config_dir: fake_path.clone(),
        config_file: fake_config_file,
    };

    let result = initializer.create_config_directory();
    assert!(result.is_err());

    match result.unwrap_err() {
        ConfigError::DirectoryCreationFailed { path, source } => {
            assert_eq!(path, fake_path);
            // Should be a permission denied or not found error
            assert!(matches!(
                source.kind(),
                io::ErrorKind::PermissionDenied
                    | io::ErrorKind::NotFound
                    | io::ErrorKind::ReadOnlyFilesystem
                    | io::ErrorKind::Other // Some systems may return Other for this case
            ));
        }
        other => panic!("Expected DirectoryCreationFailed error, got {:?}", other),
    }
}

#[test]
fn test_create_template_file_permission_denied() {
    // Similar to directory creation, this is challenging to test portably
    // We'll create a directory but make it so we can't write to it
    let temp_dir = tempdir().unwrap();
    let config_dir = temp_dir.path().join("tmux-sessionizer");
    fs::create_dir_all(&config_dir).unwrap();

    // On Unix systems, we could try to make the directory read-only
    // But this test may not work reliably across all platforms
    // For now, we'll test with a path that should fail

    let impossible_config_file = PathBuf::from("/dev/null/cannot_create_file_here");

    let initializer = ConfigInitializer {
        config_dir: config_dir.clone(),
        config_file: impossible_config_file.clone(),
    };

    let result = initializer.create_template_file();
    assert!(result.is_err());

    match result.unwrap_err() {
        ConfigError::TemplateWriteFailed { path, .. } => {
            assert_eq!(path, impossible_config_file);
        }
        other => panic!("Expected TemplateWriteFailed error, got {:?}", other),
    }
}

#[test]
fn test_validate_created_file_path_is_directory() {
    let temp_dir = tempdir().unwrap();
    let config_dir = temp_dir.path().join("tmux-sessionizer");
    fs::create_dir_all(&config_dir).unwrap();

    // Create a directory where the file should be
    let fake_file_path = config_dir.join("tmux-sessionizer.toml");
    fs::create_dir(&fake_file_path).unwrap();

    let initializer = ConfigInitializer {
        config_dir,
        config_file: fake_file_path.clone(),
    };

    let result = initializer.validate_created_file();
    assert!(result.is_err());

    match result.unwrap_err() {
        ConfigError::ValidationFailed { path, .. } => {
            assert_eq!(path, fake_file_path);
        }
        other => panic!("Expected ValidationFailed error, got {:?}", other),
    }
}

#[test]
fn test_config_initializer_new_xdg_error() {
    // This test attempts to trigger a failure in BaseDirs::new()
    // by temporarily unsetting environment variables that XDG depends on

    let original_home = env::var_os("HOME");
    let original_xdg_config_home = env::var_os("XDG_CONFIG_HOME");

    // Remove environment variables that might be needed for XDG
    unsafe {
        env::remove_var("HOME");
        env::remove_var("XDG_CONFIG_HOME");
    }

    let result = ConfigInitializer::new();

    // Restore environment variables
    if let Some(val) = original_home {
        unsafe {
            env::set_var("HOME", val);
        }
    }
    if let Some(val) = original_xdg_config_home {
        unsafe {
            env::set_var("XDG_CONFIG_HOME", val);
        }
    }

    // On some systems this might still succeed if there are fallbacks
    // but if it fails, it should be with the right error type
    if let Err(error) = result {
        match error {
            ConfigError::CannotDetermineConfigDir => {
                // This is the expected error
            }
            other => panic!("Expected CannotDetermineConfigDir error, got {:?}", other),
        }
    }
    // If it succeeds, that's fine too - the system has fallbacks
}
