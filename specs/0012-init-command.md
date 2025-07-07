# Spec 0012: Add --init Command

## Summary

Add a `--init` command-line flag that creates the configuration directory structure and generates a commented configuration file template at `~/.config/tmux-sessionizer/tmux-sessionizer.toml`.

## Motivation

Currently, users must manually create the configuration directory and file structure to customize tmux-sessionizer behavior. This creates friction for new users who want to:
- Quickly set up custom search paths
- Configure exclusion patterns
- See available configuration options

The `--init` command will streamline the initial setup process by automatically creating the necessary directory structure and providing a fully-commented configuration template.

## Design

### Command-Line Interface

Add a new `--init` flag to the existing CLI:

```bash
tmux-sessionizer --init [--debug]
```

- `--init`: Create configuration directory and template file
- `--debug`: Optional flag for verbose logging during initialization

### Behavior Specification

1. **Directory Creation**: 
   - Create `~/.config/tmux-sessionizer/` directory if it doesn't exist
   - Use cross-platform XDG config directory resolution (already implemented via `cross-xdg` crate)

2. **Config File Creation**:
   - Generate `tmux-sessionizer.toml` from the existing `examples/tmux-sessionizer.toml` template
   - Comment out all configuration values (prefix lines with `#`)
   - Preserve all explanatory comments and structure

3. **Conflict Resolution**:
   - If config file already exists: do nothing, exit with success message
   - If directory exists but is empty: create the config file
   - If directory creation fails: exit with error

4. **User Feedback**:
   - Print success message showing created file path
   - Validate that file was created successfully
   - Support `--debug` flag for verbose output

5. **Exit Behavior**:
   - `--init` is mutually exclusive with normal operation
   - After successful initialization, exit immediately
   - Do not proceed with directory scanning or fuzzy finder

### Implementation Details

#### CLI Arguments Update

Update `CliArgs` struct in `src/config.rs`:

```rust
#[derive(Parser, Debug)]
pub(crate) struct CliArgs {
    /// Enable detailed debug logging.
    #[arg(short, long, action = clap::ArgAction::SetTrue)]
    debug: bool,
    
    /// Initialize configuration directory and create template config file.
    #[arg(long, action = clap::ArgAction::SetTrue)]
    init: bool,
    
    /// Directly select a path or name, skipping the fuzzy finder.
    #[arg(index = 1)]
    direct_selection: Option<String>,
}
```

#### Config Template Generation

Create new module `src/config_init.rs`:

```rust
pub struct ConfigInitializer {
    config_dir: PathBuf,
    config_file: PathBuf,
}

impl ConfigInitializer {
    pub fn new() -> Result<Self, ConfigError>;
    pub fn init_config(&self) -> Result<(), ConfigError>;
    fn create_config_directory(&self) -> Result<(), ConfigError>;
    fn create_template_file(&self) -> Result<(), ConfigError>;
    fn generate_template_content() -> String;
    fn validate_created_file(&self) -> Result<(), ConfigError>;
}
```

#### Template Content

The template will be based on `examples/tmux-sessionizer.toml` with all non-comment lines prefixed with `#`:

```toml
# Example configuration file for tmux-sessionizer
# 
# This file allows you to customize the behavior of tmux-sessionizer,
# such as specifying default search directories and exclusion patterns.

# --- Search Paths ---
# search_paths = [
#   "~/dev",
#   "~/workspaces",
# ]

# --- Additional Paths ---
# additional_paths = [
#   "~/clients/project-x",
#   "/mnt/shared/team-projects",
# ]

# --- Exclusion Patterns ---
# exclude_patterns = [
#   "/node_modules/",
#   "/target/",
#   "/vendor/",
# ]
```

#### Error Handling

New error variants in `src/error.rs`:

```rust
#[derive(Error, Debug)]
pub enum ConfigError {
    // ... existing variants ...
    
    #[error("Failed to create config directory: {path}")]
    DirectoryCreationFailed {
        path: PathBuf,
        source: io::Error,
    },
    
    #[error("Failed to write config template: {path}")]
    TemplateWriteFailed {
        path: PathBuf,
        source: io::Error,
    },
    
    #[error("Config file validation failed: {path}")]
    ValidationFailed {
        path: PathBuf,
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}
```

## Test-Driven Development Plan

Follow RED/GREEN/REFACTOR cycles, implementing one test at a time. **Check off each item as work is accomplished:**

### Cycle 1: CLI Argument Parsing
- [ ] **RED**: Write failing test for `--init` flag parsing
- [ ] **GREEN**: Implement CLI argument parsing for `--init`
- [ ] **REFACTOR**: Clean up argument parsing code if needed

### Cycle 2: Config Directory Creation
- [ ] **RED**: Write failing test for config directory creation
- [ ] **GREEN**: Implement directory creation logic
- [ ] **REFACTOR**: Improve error handling and path resolution

### Cycle 3: Template File Generation
- [ ] **RED**: Write failing test for template file creation
- [ ] **GREEN**: Implement template generation and file writing
- [ ] **REFACTOR**: Extract template content to constants

### Cycle 4: Integration Workflow
- [ ] **RED**: Write failing integration test for complete `--init` workflow
- [ ] **GREEN**: Wire up main.rs to handle `--init` flag
- [ ] **REFACTOR**: Improve user feedback and logging

### Cycle 5: Error Handling
- [ ] **RED**: Write failing tests for error scenarios (permissions, existing files)
- [ ] **GREEN**: Implement robust error handling
- [ ] **REFACTOR**: Consolidate error types and improve messages

## Acceptance Criteria

1. **Command Execution**: `tmux-sessionizer --init` creates config directory and template file
2. **Directory Handling**: Creates `~/.config/tmux-sessionizer/` if it doesn't exist
3. **File Creation**: Generates `tmux-sessionizer.toml` with commented template
4. **Conflict Resolution**: Does nothing if config file already exists
5. **User Feedback**: Shows success message with created file path
6. **Validation**: Verifies created file exists and is readable
7. **Debug Support**: `--debug` flag provides verbose initialization logging
8. **Exit Behavior**: Command exits after initialization, doesn't proceed with normal operation
9. **Error Handling**: Graceful handling of permission errors and filesystem issues
10. **Template Quality**: Generated template matches `examples/tmux-sessionizer.toml` structure with all values commented out

## Dependencies

- No new external dependencies required
- Leverages existing `cross-xdg`, `std::fs`, and `clap` functionality
- Uses existing error handling patterns from `src/error.rs`

## Future Considerations

- Could add `--init --force` flag to overwrite existing config files
- Could add `--init --minimal` for a smaller template
- Could add config validation/linting commands
- Could add `--init --interactive` for guided configuration setup