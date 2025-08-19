use crate::error::ConfigError;
use cross_xdg::BaseDirs;
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info};

#[derive(Debug)]
pub struct ConfigInitializer {
    config_dir: PathBuf,
    config_file: PathBuf,
}

impl ConfigInitializer {
    pub fn new() -> Result<Self, ConfigError> {
        let base_dirs = BaseDirs::new().map_err(|_| ConfigError::CannotDetermineConfigDir)?;

        let config_dir = base_dirs.config_home().join("tmux-sessionizer");
        let config_file = config_dir.join("tmux-sessionizer.toml");

        Ok(ConfigInitializer {
            config_dir,
            config_file,
        })
    }

    pub fn config_file(&self) -> &PathBuf {
        &self.config_file
    }

    pub fn init_config(&self) -> Result<bool, ConfigError> {
        self.create_config_directory()?;
        let file_was_created = self.create_template_file()?;
        self.validate_created_file()?;
        Ok(file_was_created)
    }

    fn create_config_directory(&self) -> Result<(), ConfigError> {
        if self.config_dir.exists() {
            debug!(
                "Config directory already exists: {}",
                self.config_dir.display()
            );
        } else {
            debug!("Creating config directory: {}", self.config_dir.display());
            fs::create_dir_all(&self.config_dir).map_err(|e| {
                ConfigError::DirectoryCreationFailed {
                    path: self.config_dir.clone(),
                    source: e,
                }
            })?;
            info!("Created config directory: {}", self.config_dir.display());
        }
        Ok(())
    }

    fn create_template_file(&self) -> Result<bool, ConfigError> {
        if self.config_file.exists() {
            info!("Config file already exists: {}", self.config_file.display());
            return Ok(false); // File was not created, it already existed
        }

        debug!(
            "Creating config template file: {}",
            self.config_file.display()
        );
        let template_content = Self::generate_template_content();
        fs::write(&self.config_file, template_content).map_err(|e| {
            ConfigError::TemplateWriteFailed {
                path: self.config_file.clone(),
                source: e,
            }
        })?;
        info!(
            "Created config template file: {}",
            self.config_file.display()
        );
        Ok(true) // File was created
    }

    fn generate_template_content() -> String {
        // Based on examples/tmux-sessionizer.toml with all configuration lines commented out
        r#"# Example configuration file for tmux-sessionizer
#
# This file allows you to customize the behavior of tmux-sessionizer,
# such as specifying default search directories and exclusion patterns.
#
# The configuration file should be placed at:
# ~/.config/tmux-sessionizer/tmux-sessionizer.toml

# --- Search Paths ---
#
# `search_paths` defines the primary directories where tmux-sessionizer will look for projects.
# These paths are scanned recursively up to a certain depth (currently hardcoded, but could be configurable).
# Paths starting with '~' will be expanded to the user's home directory.
#
# Example: Search within ~/dev and ~/workspaces
# search_paths = [
#   "~/dev",
#   "~/workspaces",
# ]
#
# Example: Search only in a specific project directory
# search_paths = ["/path/to/my/projects"]


# --- Default New Project Path ---
#
# `default_new_project_path` specifies where new projects should be created when using the
# "create new project" feature from the fuzzy finder interface. This path will be expanded
# if it starts with '~'. Defaults to ~/dev if not specified.
#
# Example: Create new projects in ~/projects
# default_new_project_path = "~/projects"
#
# Example: Create new projects in a specific directory
# default_new_project_path = "/path/to/my/projects"


# --- Additional Paths ---
#
# `additional_paths` allows specifying extra directories to include in the scan.
# These are treated similarly to `search_paths`. This can be useful for adding
# temporary or less frequently accessed project locations without modifying the main search paths.
# Paths starting with '~' will be expanded.
#
# Example: Include a specific client project directory
# additional_paths = [
#   "~/clients/project-x",
#   "/mnt/shared/team-projects",
# ]


# --- Exclusion Patterns ---
#
# `exclude_patterns` is a list of regular expressions. Any directory whose *full path*
# matches one of these patterns will be excluded from the results.
# Use this to ignore common directories like node_modules, target, vendor, etc.,
# or specific projects you don't want to appear in the list.
#
# Note: The patterns are matched against the absolute path.
# Remember to escape special regex characters if needed (e.g., '.' should be '\.').
#
# Example: Exclude common build/dependency directories and hidden directories
# exclude_patterns = [
#   "/node_modules/", # Exclude node_modules anywhere in the path
#   "/target/",       # Exclude Rust target directories
#   "/vendor/",       # Exclude vendor directories (e.g., PHP Composer, Go)
#   "/\.git/",        # Exclude the .git directory itself (though we mainly look for dirs containing .git)
#   "/\.cache/",      # Exclude common cache directories
#   "/__pycache__/",  # Exclude Python bytecode cache
#   "/\.venv/",       # Exclude Python virtual environments
#   "/\.svn/",        # Exclude Subversion metadata directories
#   "/\.hg/",         # Exclude Mercurial metadata directories
# ]
#
# Example: Exclude a specific project by path
# exclude_patterns = [
#   "/path/to/my/projects/archived-project",
# ]
#
# Example: Combine common exclusions and specific ones
# exclude_patterns = [
#   "/node_modules/",
#   "/target/",
#   "~/dev/legacy-project", # Exclude a specific project in the dev directory
# ]
"#.to_string()
    }

    fn validate_created_file(&self) -> Result<(), ConfigError> {
        if !self.config_file.exists() {
            return Err(ConfigError::ValidationFailed {
                path: self.config_file.clone(),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Config file was not created successfully",
                )),
            });
        }

        if !self.config_file.is_file() {
            return Err(ConfigError::ValidationFailed {
                path: self.config_file.clone(),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Config file path exists but is not a file",
                )),
            });
        }

        debug!(
            "Config file validation passed: {}",
            self.config_file.display()
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests;
