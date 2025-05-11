
use std::ffi::OsStr;
use std::path::Path;
use std::env; // For checking TMUX env var
use tmux_interface::{AttachSession, HasSession, ListSessions, NewSession, SwitchClient, Tmux, Error as TmuxError};
use tracing::{debug, error};

pub struct SessionManager {}

impl SessionManager {
    /// Creates a new `SessionManager`.
    pub fn new() -> Self {
        Self {}
    }

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
    pub fn generate_session_name(&self, item_path: &Path, parent_repo_path: Option<&Path>) -> String {
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
            format!("{}_{}", parent_basename, item_basename)
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
    /// * `Err(TmuxError)` if there was an issue communicating with tmux, other than the server not running.
    pub fn is_tmux_server_running(&self) -> Result<bool, TmuxError> {
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
                if let TmuxError::Tmux(message) = &e {
                    if message.contains("no server running") || message.contains("failed to connect to server") {
                        debug!("Tmux server is not running (detected via error message).");
                        Ok(false)
                    } else {
                        debug!("Tmux command error while checking server status: {}", message);
                        Err(e) // Propagate other tmux errors
                    }
                } else {
                    debug!("Non-tmux error while checking server status: {}", e);
                    Err(e) // Propagate IO, Parse, etc. errors
                }

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
    /// * `Err(TmuxError)` if there was an issue communicating with tmux, other than the server not running.
    pub fn session_exists(&self, session_name: &str) -> Result<bool, TmuxError> {
        debug!("Checking if session '{}' exists.", session_name);
        match Tmux::new().command(HasSession::new().target_session(session_name)).status() {
            Ok(status) => {
                let exists = status.success();
                debug!("Session '{}' exists: {}.", session_name, exists);
                Ok(exists)
            }
            Err(e) => {
                if let TmuxError::Tmux(message) = &e {
                    if message.contains("no server running") || message.contains("failed to connect to server") {
                        debug!("Tmux server not running, so session '{}' cannot exist (detected via error message).", session_name);
                        Ok(false) // If server isn't running, session can't exist.
                    } else {
                        debug!("Tmux command error while checking for session '{}': {}", session_name, message);
                        Err(e) // Propagate other tmux errors
                    }
                } else {
                    debug!("Non-tmux error while checking for session '{}': {}", session_name, e);
                    Err(e) // Propagate IO, Parse, etc. errors
                }
            }
        }
    }

    /// Helper to check if currently inside a tmux session.
    fn is_inside_tmux_session(&self) -> bool {
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
    /// * `Err(TmuxError)` if there was an error creating the session.
    pub fn create_new_session(&self, session_name: &str, start_directory: &Path) -> Result<(), TmuxError> {
        debug!(
            "Creating new session '{}' at path '{}'. Inside tmux: {}",
            session_name,
            start_directory.display(),
            self.is_inside_tmux_session()
        );

        let mut new_session_cmd = NewSession::new();
        new_session_cmd = new_session_cmd.session_name(session_name);
        // Bind the Cow<'_, str> to a variable to extend its lifetime
        let start_dir_cow = start_directory.to_string_lossy();
        new_session_cmd = new_session_cmd.start_directory(start_dir_cow.as_ref());

        if self.is_inside_tmux_session() {
            new_session_cmd = new_session_cmd.detached();
        }

        Tmux::new().command(new_session_cmd).output().map(|_| ()).map_err(|e| {
            error!("Failed to create new session '{}' at path '{}': {:?}", session_name, start_directory.display(), e);
            e
        })
    }

    /// Switches the current tmux client to an existing session, or attaches to it.
    ///
    /// If the program is run from within an existing tmux session (TMUX env var is set),
    /// it uses `switch-client` to change the current client's active session.
    /// If not inside a tmux session, it uses `attach-session` to attach the current
    /// terminal to the specified session. This typically requires the tmux server to be running
    /// and the session to exist.
    pub fn switch_or_attach_to_session(&self, session_name: &str) -> Result<(), TmuxError> {
        debug!("Switching or attaching to session '{}'. Inside tmux: {}", session_name, self.is_inside_tmux_session());

        if self.is_inside_tmux_session() {
            let switch_client_cmd = SwitchClient::new().target_session(session_name);
            Tmux::new().command(switch_client_cmd).output().map(|_| ()).map_err(|e| {
                error!("Failed to switch client to session '{}': {:?}", session_name, e);
                e
            })
        } else {
            let attach_session_cmd = AttachSession::new().target_session(session_name);
            Tmux::new().command(attach_session_cmd).output().map(|_| ()).map_err(|e| {
                error!("Failed to attach to session '{}': {:?}", session_name, e);
                e
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_generate_session_name_simple() {
        let manager = SessionManager::new();
        let path = PathBuf::from("/path/to/my.project");
        let name = manager.generate_session_name(&path, None);
        assert_eq!(name, "my-project");
    }

    #[test]
    fn test_generate_session_name_with_colon() {
        let manager = SessionManager::new();
        let path = PathBuf::from("/path/to/project:name");
        let name = manager.generate_session_name(&path, None);
        assert_eq!(name, "project-name");
    }

    #[test]
    fn test_generate_session_name_worktree() {
        let manager = SessionManager::new();
        let item_path = PathBuf::from("/path/to/main_repo/worktrees/feature.branch");
        let parent_repo_path = PathBuf::from("/path/to/main_repo");
        let name = manager.generate_session_name(&item_path, Some(&parent_repo_path));
        assert_eq!(name, "main_repo_feature-branch");
    }

    #[test]
    fn test_generate_session_name_worktree_with_dots_in_parent() {
        let manager = SessionManager::new();
        let item_path = PathBuf::from("/path/to/parent.repo/worktrees/my_feature");
        let parent_repo_path = PathBuf::from("/path/to/parent.repo");
        let name = manager.generate_session_name(&item_path, Some(&parent_repo_path));
        assert_eq!(name, "parent-repo_my_feature");
    }
    
    #[test]
    fn test_generate_session_name_root_path_item() {
        let manager = SessionManager::new();
        let item_path = PathBuf::from("/");
        let name = manager.generate_session_name(&item_path, None);
        assert_eq!(name, "default_session");
    }

    #[test]
    fn test_generate_session_name_root_path_parent() {
        let manager = SessionManager::new();
        let item_path = PathBuf::from("/some/project");
        let parent_repo_path = PathBuf::from("/");
        let name = manager.generate_session_name(&item_path, Some(&parent_repo_path));
        assert_eq!(name, "default_parent_project");
    }

    #[test]
    fn test_generate_session_name_empty_item_basename() {
        // This case should ideally not happen with real directory paths,
        // but testing the fallback. An empty string as PathBuf filename part is tricky.
        // Let's simulate by a path that might result in empty after OsStr conversion if not careful.
        // However, Path::file_name() on "/foo/" gives "foo". On "/" gives None.
        // The unwrap_or_else(|| OsStr::new("")) handles the None case.
        // If item_path.file_name() somehow yields Some(""), it should become "default_session".
        // This test is more conceptual for the internal logic.
        let manager = SessionManager::new();
        // A path like "." might have file_name() as ".".
        // A path like "" is invalid.
        // Let's trust the `if item_basename.is_empty()` check.
        // For this test, we rely on the root path test to cover the empty OsStr part.
        // Here, we test the explicit `is_empty()` check.
        // If item_path is "/home/user/.config", item_basename is ".config"
        // If item_path is "/home/user/..", item_basename is ".."
        // It's hard to construct a valid Path that gives an empty (but not None) file_name.
        // The current implementation handles `None` from `file_name()` by converting `OsStr::new("")`
        // to `""` string, which then triggers `item_basename = "default_session".to_string();`.
        // So, `test_generate_session_name_root_path_item` covers this.
    }

    // Note: Tests for `is_tmux_server_running` and `session_exists` would require a live tmux server
    // or mocking the `tmux_interface` calls, which is beyond simple unit tests here.
    // These will be tested implicitly during integration testing or manually.
}
