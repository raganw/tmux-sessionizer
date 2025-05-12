use crate::directory_scanner::DirectoryEntry; // For creating Selection
use crate::error::{AppError, Result}; // Add AppError here
use std::env; // For checking TMUX env var
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use tmux_interface::{
    AttachSession, Error as TmuxInterfaceError, HasSession, ListSessions, NewSession, SwitchClient,
    Tmux,
}; // Aliased Error
use tracing::{debug, error};

pub struct SessionManager {}

/// Represents a user's chosen directory, ready for session management.
#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    /// The canonical filesystem path to the selected directory.
    pub path: PathBuf,
    /// The name displayed to the user in the fuzzy finder.
    pub display_name: String,
    /// The generated or existing tmux session name for this selection.
    pub session_name: String,
}

impl SessionManager {
    /// Generates a sanitized tmux session name.
    ///
    /// For standard paths, the session name is derived from the directory's base name.
    /// For worktrees, it's formatted as `parent_basename_worktree_basename`.
    /// Characters like `.` and `:` are replaced with `-`.
    ///
    /// # Arguments
    ///
    /// * `item_path`: The path to the directory for which to generate the session name.
    /// * `parent_repo_path`: Optional. If `item_path` refers to a worktree, this should be
    ///   the path to its parent repository. This is used to construct the `parent_child`
    ///   session name format.
    ///
    /// # Returns
    ///
    /// A `String` suitable for use as a tmux session name.
    pub fn generate_session_name(
        // &self, // Removed
        item_path: &Path,
        parent_repo_path: Option<&Path>,
    ) -> String {
        let item_basename_osstr = item_path.file_name().unwrap_or_else(|| OsStr::new(""));
        let mut item_basename = item_basename_osstr.to_string_lossy().into_owned();
        if item_basename.is_empty() || item_basename == "/" {
            item_basename = "default_session".to_string();
        }

        let raw_name = if let Some(parent_path) = parent_repo_path {
            let parent_basename_osstr = parent_path.file_name().unwrap_or_else(|| OsStr::new(""));
            let mut parent_basename = parent_basename_osstr.to_string_lossy().into_owned();
            if parent_basename.is_empty() || parent_basename == "/" {
                parent_basename = "default_parent".to_string();
            }
            format!("{parent_basename}_{item_basename}")
        } else {
            item_basename
        };

        // Sanitize the raw name: replace '.' and ':' with '-'
        let sanitized_name = raw_name.replace(['.', ':'], "-");
        debug!(
            "Generated session name: '{}' from item_path: '{}', parent_repo_path: '{:?}'",
            sanitized_name,
            item_path.display(),
            parent_repo_path.map(|p| p.display())
        );
        sanitized_name
    }

    /// Checks if a tmux server is currently running.
    ///
    /// # Returns
    ///
    /// * `Ok(true)` if a tmux server is running.
    /// * `Ok(false)` if no tmux server is running.
    /// * `Err(AppError::TmuxError)` if there was an issue communicating with tmux, other than the server not running.
    pub fn is_tmux_server_running() -> Result<bool> {
        debug!("Checking if tmux server is running.");
        // Attempt a benign command like listing sessions.
        // If it succeeds, server is running.
        // If it fails with a "server not running" error, server is not running.
        // Other errors are propagated.
        match Tmux::new().command(ListSessions::new()).output() {
            Ok(_) => {
                debug!("Tmux server is running.");
                Ok(true)
            }
            Err(e) => {
                // Check if the error is specifically a TmuxInterfaceError::Tmux variant with a relevant message
                if let TmuxInterfaceError::Tmux(ref message) = e {
                    if message.contains("no server running")
                        || message.contains("failed to connect to server")
                    {
                        debug!("Tmux server is not running (detected via error message).");
                        return Ok(false);
                    }
                }
                // If it's not the specific "no server" message, or a different type of error, propagate it.
                debug!("Error while checking server status: {}", e);
                Err(e.into()) // Convert to AppError before returning
            }
        }
    }

    /// Checks if a specific tmux session exists.
    ///
    /// # Arguments
    ///
    /// * `session_name`: The name of the tmux session to check.
    ///
    /// # Returns
    ///
    /// * `Ok(true)` if the session exists.
    /// * `Ok(false)` if the session does not exist or if the tmux server is not running.
    /// * `Err(AppError::TmuxError)` if there was an issue communicating with tmux, other than the server not running.
    pub fn session_exists(session_name: &str) -> Result<bool> {
        debug!("Checking if session '{}' exists.", session_name);
        match Tmux::new()
            .command(HasSession::new().target_session(session_name))
            .status()
        {
            Ok(status) => {
                let exists = status.success();
                debug!("Session '{}' exists: {}.", session_name, exists);
                Ok(exists)
            }
            Err(e) => {
                // Check if the error is specifically a TmuxInterfaceError::Tmux variant with a relevant message
                if let TmuxInterfaceError::Tmux(ref message) = e {
                    if message.contains("no server running")
                        || message.contains("failed to connect to server")
                    {
                        debug!(
                            "Tmux server not running, so session '{}' cannot exist (detected via error message).",
                            session_name
                        );
                        return Ok(false); // If server isn't running, session can't exist.
                    }
                }
                debug!("Error while checking for session '{}': {}", session_name, e);
                Err(e.into()) // Convert to AppError before returning
            }
        }
    }

    /// Helper to check if currently inside a tmux session.
    fn is_inside_tmux_session() -> bool {
        env::var("TMUX").is_ok()
    }

    /// Creates a new tmux session.
    ///
    /// If not already inside a tmux session (TMUX env var is not set),
    /// this new session will be attached to the current terminal.
    /// If inside an existing tmux session, this new session will be created detached.
    /// In the latter case, `switch_or_attach_to_session` might be needed subsequently
    /// if an immediate switch to the new session is desired.
    ///
    /// # Arguments
    ///
    /// * `session_name`: The desired name for the new tmux session.
    /// * `start_directory`: The directory where the new session should start.
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the session was created successfully.
    /// * `Err(AppError::TmuxError)` if there was an error creating the session.
    pub fn create_new_session(session_name: &str, start_directory: &Path) -> Result<()> {
        debug!(
            "Creating new session '{}' at path '{}'. Inside tmux: {}",
            session_name,
            start_directory.display(),
            Self::is_inside_tmux_session()
        );

        let mut new_session_cmd = NewSession::new();
        new_session_cmd = new_session_cmd.session_name(session_name);
        // Bind the Cow<'_, str> to a variable to extend its lifetime
        let start_dir_cow = start_directory.to_string_lossy();
        new_session_cmd = new_session_cmd.start_directory(start_dir_cow.as_ref());

        if Self::is_inside_tmux_session() {
            new_session_cmd = new_session_cmd.detached();
        }

        Tmux::new()
            .command(new_session_cmd)
            .output()
            .map(|_| ())
            .map_err(|e| {
                let err_msg = format!(
                    "Failed to create new session '{}' for directory '{}': {}",
                    session_name,
                    start_directory.display(),
                    e
                );
                error!("{}", err_msg); // Log the detailed error
                AppError::Session(err_msg)
            })
    }

    /// Switches the current tmux client to an existing session, or attaches to it.
    ///
    /// If the program is run from within an existing tmux session (TMUX env var is set),
    /// it uses `switch-client` to change the current client's active session.
    /// If not inside a tmux session, it uses `attach-session` to attach the current
    /// terminal to the specified session. This typically requires the tmux server to be running
    /// and the session to exist.
    pub fn switch_or_attach_to_session(session_name: &str) -> Result<()> {
        debug!(
            "Switching or attaching to session '{}'. Inside tmux: {}",
            session_name,
            Self::is_inside_tmux_session()
        );

        if Self::is_inside_tmux_session() {
            let switch_client_cmd = SwitchClient::new().target_session(session_name);
            Tmux::new()
                .command(switch_client_cmd)
                .output()
                .map(|_| ())
                .map_err(|e| {
                    let err_msg =
                        format!("Failed to switch client to session '{session_name}': {e}",);
                    error!("{}", err_msg);
                    AppError::Session(err_msg)
                })
        } else {
            let attach_session_cmd = AttachSession::new().target_session(session_name);
            Tmux::new()
                .command(attach_session_cmd)
                .output()
                .map(|_| ())
                .map_err(|e| {
                    let err_msg = format!("Failed to attach to session '{session_name}': {e}");
                    error!("{}", err_msg);
                    AppError::Session(err_msg)
                })
        }
    }

    /// Creates a `Selection` struct from a `DirectoryEntry`.
    ///
    /// This involves determining the final path, display name, and generating
    /// the appropriate tmux session name.
    ///
    /// # Arguments
    ///
    /// * `dir_entry`: A reference to the `DirectoryEntry` chosen by the user.
    ///
    /// # Returns
    ///
    /// A `Selection` struct populated with details from the `DirectoryEntry`.
    pub fn create_selection_from_directory_entry(dir_entry: &DirectoryEntry) -> Selection {
        let session_name =
            Self::generate_session_name(&dir_entry.resolved_path, dir_entry.parent_path.as_deref());
        Selection {
            path: dir_entry.resolved_path.clone(),
            display_name: dir_entry.display_name.clone(),
            session_name,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    // Note: Tests for `is_tmux_server_running` and `session_exists` would require a live tmux server
    // or mocking the `tmux_interface` calls, which is beyond simple unit tests here.
    // These will be tested implicitly during integration testing or manually.
}
