use super::*;
use std::env;
use std::path::PathBuf;

#[test]
fn test_expand_tilde_no_tilde() {
    let path = PathBuf::from("/var/tmp/project");
    assert_eq!(expand_tilde(&path), Some(path));
}

#[test]
fn test_expand_tilde_just_tilde() {
    // This test relies on dirs::home_dir() returning Some(path)
    // and that path being what we expect.
    if let Some(home) = dirs::home_dir() {
        assert_eq!(expand_tilde(&PathBuf::from("~")), Some(home));
    } else {
        // If home_dir is None, expand_tilde should also be None.
        assert_eq!(expand_tilde(&PathBuf::from("~")), None);
    }
}

#[test]
fn test_expand_tilde_with_path() {
    if let Some(mut home) = dirs::home_dir() {
        home.push("some_project");
        assert_eq!(expand_tilde(&PathBuf::from("~/some_project")), Some(home));
    } else {
        assert_eq!(expand_tilde(&PathBuf::from("~/some_project")), None);
    }
}

#[test]
fn test_expand_tilde_with_trailing_slash() {
    if let Some(mut home) = dirs::home_dir() {
        home.push("some_project"); // PathBuf join handles this correctly
        assert_eq!(expand_tilde(&PathBuf::from("~/some_project/")), Some(home));
    } else {
        assert_eq!(expand_tilde(&PathBuf::from("~/some_project/")), None);
    }
}

#[test]
fn test_expand_tilde_no_home_dir_scenario() {
    // Temporarily unset HOME environment variable to simulate no home dir.
    // This is platform-dependent and might not work everywhere or might be flaky.
    let original_home = env::var_os("HOME");
    // SAFETY: Modifying environment variables is unsafe. This is a test
    // specifically designed to alter the HOME variable temporarily.
    unsafe {
        env::remove_var("HOME");
    }

    // On some systems, dirs::home_dir() might still find a home (e.g., from /etc/passwd).
    // This test is more of a best-effort for typical Unix-like systems where HOME matters most.
    // If dirs::home_dir() still returns Some(), this test might not behave as expected for "no home"
    // but will test expand_tilde's behavior given what dirs::home_dir() provides.

    if dirs::home_dir().is_none() {
        // Only assert if we truly simulated no home dir
        assert_eq!(expand_tilde(&PathBuf::from("~/Documents")), None);
    }

    // Restore HOME
    if let Some(home_val) = original_home {
        // SAFETY: Restoring the HOME environment variable. This is part of
        // the test's controlled environment manipulation.
        unsafe {
            env::set_var("HOME", home_val);
        }
    }
}
