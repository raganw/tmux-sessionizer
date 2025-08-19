use crate::directory_scanner::DirectoryEntry;
use crate::error::{AppError, Result};
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use tmux_interface::{
    AttachSession, Error as TmuxInterfaceError, HasSession, ListSessions, NewSession, SwitchClient,
    Tmux,
};
use tracing::{debug, error, info};

/// Provides methods for interacting with tmux sessions.
///
/// This struct is currently a placeholder for namespacing session management functions.
/// It does not hold any state itself, but its methods operate on tmux sessions.
pub struct SessionManager {}

/// Represents a user's chosen directory, ready for session management.
///
/// This struct holds the necessary information to identify and manage a tmux session
/// corresponding to a selected directory or Git worktree.
#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    /// The canonical, absolute filesystem path to the selected directory or worktree.
    pub path: PathBuf,
    /// The name that was displayed to the user in the fuzzy finder interface.
    pub display_name: String,
    /// The generated or existing tmux session name corresponding to this selection.
    /// This name is sanitized to be compatible with tmux session naming rules.
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
    /// A sanitized `String` suitable for use as a tmux session name.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::path::Path;
    /// # use tmux_sessionizer::session_manager::SessionManager;
    /// let path = Path::new("/home/user/projects/my.project");
    /// assert_eq!(SessionManager::generate_session_name(path, None), "my-project");
    ///
    /// let worktree_path = Path::new("/repos/main.repo/worktrees/feature-branch");
    /// let parent_path = Path::new("/repos/main.repo");
    /// assert_eq!(SessionManager::generate_session_name(worktree_path, Some(parent_path)), "main-repo_feature-branch");
    /// ```
    pub fn generate_session_name(item_path: &Path, parent_repo_path: Option<&Path>) -> String {
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
    /// * `Ok(true)` if a tmux server is running and responsive.
    /// * `Ok(false)` if no tmux server is running.
    /// * `Err(AppError::Tmux)` if there was an issue communicating with tmux (e.g., permission errors, unexpected output), other than the server simply not running.
    pub fn is_tmux_server_running() -> Result<bool> {
        debug!("Checking if tmux server is running.");
        // Attempt a benign command like listing sessions.
        // Success implies a running server.
        // Specific error messages indicate a non-running server.
        // Other errors are treated as communication problems.
        match Tmux::new().command(ListSessions::new()).output() {
            Ok(_) => {
                debug!("Tmux server is running (ListSessions succeeded).");
                Ok(true)
            }
            Err(e) => {
                // Check if the error message indicates the server isn't running.
                if let TmuxInterfaceError::Tmux(ref message) = e
                    && (message.contains("no server running")
                        || message.contains("failed to connect to server"))
                {
                    debug!("Tmux server is not running (detected via specific error message).");
                    return Ok(false);
                }
                // Otherwise, it's some other communication error.
                error!("Error while checking tmux server status: {}", e);
                Err(AppError::Tmux(e))
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
    /// * `Ok(true)` if a session with the exact `session_name` exists.
    /// * `Ok(false)` if the session does not exist or if the tmux server is not running (as a session cannot exist without a server).
    /// * `Err(AppError::Tmux)` if there was an issue communicating with tmux (e.g., permission errors), other than the server simply not running.
    pub fn session_exists(session_name: &str) -> Result<bool> {
        debug!("Checking if session '{}' exists.", session_name);
        // create a string that is "={session_name}"
        match Tmux::with_command(HasSession::new().target_session(format!("={session_name}")))
            .status()
        {
            Ok(status) => {
                let exists = status.success();
                debug!(
                    "Session '{}' exists check completed. Exists: {}.",
                    session_name, exists
                );
                Ok(exists)
            }
            Err(e) => {
                // Check if the error message indicates the server isn't running.
                if let TmuxInterfaceError::Tmux(ref message) = e
                    && (message.contains("no server running")
                        || message.contains("failed to connect to server"))
                {
                    debug!(
                        "Tmux server not running, thus session '{}' cannot exist (detected via specific error message).",
                        session_name
                    );
                    return Ok(false); // Session cannot exist if server isn't running.
                }
                // Otherwise, it's some other communication error.
                error!("Error while checking for session '{}': {}", session_name, e);
                Err(AppError::Tmux(e))
            }
        }
    }

    /// Checks if the application is currently running inside a tmux session
    /// by inspecting the `TMUX` environment variable.
    ///
    /// # Returns
    ///
    /// `true` if the `TMUX` environment variable is set, `false` otherwise.
    fn is_inside_tmux_session() -> bool {
        let inside = env::var("TMUX").is_ok();
        debug!("Checking if inside tmux session: {}", inside);
        inside
    }

    /// Creates a new tmux session with the specified name and starting directory.
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
    /// * `Ok(())` if the session was created successfully (either attached or detached).
    /// * `Err(AppError::Session)` if the `tmux new-session` command failed.
    pub fn create_new_session(session_name: &str, start_directory: &Path) -> Result<()> {
        debug!(
            "Attempting to create new session '{}' at path '{}'. Inside tmux: {}",
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
            .map(|_| {
                debug!(
                    "Successfully executed tmux command for creating session '{}'.",
                    session_name
                );
            })
            .map_err(|e| {
                let err_msg = format!(
                    "Failed to create new tmux session '{}' for directory '{}': {}",
                    session_name,
                    start_directory.display(),
                    e
                );
                error!("{}", err_msg);
                AppError::Session(err_msg) // Use specific AppError variant
            })
    }

    /// Switches the current tmux client to an existing session or attaches to it if outside tmux.
    ///
    /// If the program is run from within an existing tmux session (TMUX env var is set),
    /// it uses `switch-client` to change the current client's active session.
    /// If not inside a tmux session, it uses `attach-session` to attach the current terminal
    /// to the specified session.
    ///
    /// # Arguments
    ///
    /// * `session_name`: The name of the target tmux session.
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the switch or attach command was executed successfully.
    /// * `Err(AppError::Session)` if the `tmux switch-client` or `tmux attach-session` command failed.
    pub fn switch_or_attach_to_session(session_name: &str) -> Result<()> {
        debug!(
            "Attempting to switch or attach to session '{}'. Inside tmux: {}",
            session_name,
            Self::is_inside_tmux_session()
        );

        if Self::is_inside_tmux_session() {
            let switch_client_cmd = SwitchClient::new().target_session(session_name);
            Tmux::new()
                .command(switch_client_cmd)
                .output()
                .map(|_| {
                    debug!(
                        "Successfully executed switch-client to session '{}'.",
                        session_name
                    );
                })
                .map_err(|e| {
                    let err_msg =
                        format!("Failed to switch tmux client to session '{session_name}': {e}");
                    error!("{}", err_msg);
                    AppError::Session(err_msg) // Use specific AppError variant
                })
        } else {
            // Outside tmux: Attach to the session
            debug!("Executing attach-session for session '{}'.", session_name);
            let attach_session_cmd = AttachSession::new().target_session(session_name);
            Tmux::new()
                .command(attach_session_cmd)
                .output()
                .map(|_| {
                    debug!(
                        "Successfully executed attach-session for session '{}'.",
                        session_name
                    );
                })
                .map_err(|e| {
                    let err_msg = format!("Failed to attach to tmux session '{session_name}': {e}");
                    error!("{}", err_msg);
                    AppError::Session(err_msg) // Use specific AppError variant
                })
        }
    }

    /// Creates a `Selection` struct from a `DirectoryEntry` provided by the scanner.
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
    /// A `Selection` struct containing the resolved path, display name, and
    /// generated session name based on the input `DirectoryEntry`.
    pub fn create_selection_from_directory_entry(dir_entry: &DirectoryEntry) -> Selection {
        debug!(
            "Creating Selection from DirectoryEntry: path='{}', display='{}', parent='{:?}'",
            dir_entry.resolved_path.display(),
            dir_entry.display_name,
            dir_entry.parent_path.as_deref().map(|p| p.display())
        );
        let session_name =
            Self::generate_session_name(&dir_entry.resolved_path, dir_entry.parent_path.as_deref());
        Selection {
            path: dir_entry.resolved_path.clone(),
            display_name: dir_entry.display_name.clone(),
            session_name,
        }
    }

    /// Creates a new project directory and returns a `Selection` for it.
    ///
    /// This creates a new directory with the given name in the specified parent path,
    /// then creates a Selection struct that can be used to create a tmux session.
    ///
    /// # Arguments
    ///
    /// * `project_name` - The name of the new project directory to create.
    /// * `parent_path` - The parent directory where the new project should be created.
    ///
    /// # Returns
    ///
    /// A `Selection` struct for the newly created project directory.
    ///
    /// # Errors
    ///
    /// Returns `AppError::Io` if the directory cannot be created or if there are permission issues.
    pub fn create_new_project_directory(project_name: &str, parent_path: &Path) -> Result<Selection> {
        debug!(
            "Creating new project directory '{}' in '{}'",
            project_name,
            parent_path.display()
        );

        // Ensure parent directory exists
        if !parent_path.exists() {
            info!("Creating parent directory: {}", parent_path.display());
            fs::create_dir_all(parent_path).map_err(|e| {
                error!(
                    "Failed to create parent directory '{}': {}",
                    parent_path.display(),
                    e
                );
                AppError::Session(format!(
                    "Failed to create parent directory '{}': {}",
                    parent_path.display(),
                    e
                ))
            })?;
        }

        let project_path = parent_path.join(project_name);
        
        // Check if project already exists
        if project_path.exists() {
            return Err(AppError::Session(format!(
                "Project directory '{}' already exists",
                project_path.display()
            )));
        }

        // Create the project directory
        fs::create_dir(&project_path).map_err(|e| {
            error!(
                "Failed to create project directory '{}': {}",
                project_path.display(),
                e
            );
            AppError::Session(format!(
                "Failed to create project directory '{}': {}",
                project_path.display(),
                e
            ))
        })?;

        info!("Successfully created project directory: {}", project_path.display());

        // Create a Selection for the new project
        let session_name = Self::generate_session_name(&project_path, None);
        Ok(Selection {
            path: project_path,
            display_name: project_name.to_string(),
            session_name,
        })
    }
}

#[cfg(test)]
mod tests;
