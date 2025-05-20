//! Utility functions for working with file paths.

use std::path::{Path, PathBuf};
use tracing::{debug, trace};

/// Expands the tilde (`~`) character in a path to the user's home directory.
///
/// If the path does not start with `~`, it is returned as is.
/// If the home directory cannot be determined, `None` is returned.
///
/// # Arguments
///
/// * `path` - The `Path` to potentially expand.
///
/// # Returns
///
/// * `Some(PathBuf)` - The expanded path if tilde expansion was successful or not needed.
/// * `None` - If the path starts with `~` but the home directory could not be found.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use tmux_sessionizer::path_utils::expand_tilde;
///
/// // Assuming home directory is /home/user
/// if let Some(home_dir) = dirs::home_dir() {
///     let path = Path::new("~/Documents");
///     let expected = home_dir.join("Documents");
///     assert_eq!(expand_tilde(path), Some(expected));
///
///     let path_no_tilde = Path::new("/tmp/file");
///     assert_eq!(expand_tilde(path_no_tilde), Some(path_no_tilde.to_path_buf()));
///
///     let just_tilde = Path::new("~");
///     assert_eq!(expand_tilde(just_tilde), Some(home_dir));
/// } else {
///     // Test behavior when home dir is not found
///     let path = Path::new("~/Documents");
///     assert_eq!(expand_tilde(path), None);
/// }
/// ```
pub fn expand_tilde(path: &Path) -> Option<PathBuf> {
    trace!(input_path = %path.display(), "Attempting tilde expansion");
    if path.starts_with("~") {
        if let Some(home_dir) = dirs::home_dir() {
            trace!(home_dir = %home_dir.display(), "Found home directory for tilde expansion");
            let mut new_path = home_dir;
            if path.components().count() > 1 {
                // Check if there's anything after ~
                // Strip "~/" prefix and join the rest
                new_path.push(
                    path.strip_prefix("~")
                        .unwrap()
                        .strip_prefix("/")
                        .unwrap_or_else(|_| path.strip_prefix("~").unwrap()),
                );
            }
            trace!(expanded_path = %new_path.display(), "Path expanded after tilde processing");
            Some(new_path)
        } else {
            debug!(path = %path.display(), "Home directory not found for tilde expansion");
            None // Home directory could not be determined
        }
    } else {
        trace!(path = %path.display(), "Path does not start with tilde, no expansion needed");
        Some(path.to_path_buf()) // Path does not start with tilde, return as is
    }
}

#[cfg(test)]
mod tests;
