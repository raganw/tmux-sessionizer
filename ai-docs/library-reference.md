# Rust Libraries for tmux-sessionizer

This reference document provides concise information about the key libraries needed for the tmux-sessionizer Rust rewrite.

## clap - Command Line Argument Parser

A powerful and flexible library for parsing command-line arguments.

### Key Features

- Building command-line interfaces with subcommands
- Parsing options with a variety of formats (`--name value`, `--name=value`, `-n value`, etc.)
- Generating usage and help text automatically
- Validating and parsing arguments into strongly-typed values

### Basic Usage

```rust
use clap::{Arg, Command};

fn main() {
    let matches = Command::new("tmux-sessionizer")
        .version("1.0")
        .about("A utility for managing tmux sessions")
        .arg(Arg::new("debug")
            .short('d')
            .long("debug")
            .action(clap::ArgAction::SetTrue)
            .help("Enable debug mode"))
        .arg(Arg::new("path")
            .help("Direct path selection")
            .index(1))
        .get_matches();

    if matches.get_flag("debug") {
        println!("Debug mode is enabled");
    }

    if let Some(path) = matches.get_one::<String>("path") {
        println!("Selected path: {}", path);
    }
}
```

## walkdir - Directory Traversal

An efficient and cross-platform implementation of recursive directory traversal.

### Key Features

- Efficient file system traversal with minimal syscalls
- Customizable depth constraints
- Support for following symbolic links
- Filter mechanisms to exclude specific paths

### Basic Usage

```rust
use walkdir::WalkDir;
use std::path::Path;

fn scan_directories(path: &Path) {
    for entry in WalkDir::new(path)
        .min_depth(1)
        .max_depth(1)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok()) {

        println!("{}", entry.path().display());
    }
}
```

## git2 - Git Repository Operations

Rust bindings for the libgit2 library, providing functionality for working with Git repositories.

### Key Features

- Git repository discovery and manipulation
- Worktree management
- Checking repository status
- Working with references, branches, and commits

### Basic Usage

```rust
use git2::Repository;
use std::path::Path;

fn is_git_repo(path: &Path) -> bool {
    match Repository::open(path) {
        Ok(_) => true,
        Err(_) => false,
    }
}

fn get_worktrees(repo_path: &Path) -> Vec<String> {
    let repo = match Repository::open(repo_path) {
        Ok(repo) => repo,
        Err(_) => return Vec::new(),
    };

    // Example: just check if it's a bare repo that might have worktrees
    let mut worktrees = Vec::new();
    if repo.is_bare() {
        // Real implementation would list worktrees here
        worktrees.push(String::from("Example worktree"));
    }

    worktrees
}
```

## skim - Fuzzy Finder

A Rust implementation of a fuzzy finder, similar to fzf but with more Rust-native features.

### Key Features

- Interactive fuzzy search interface
- Customizable key bindings and UI
- Ability to use as a library or command-line tool
- Support for multi-selection

### Basic Usage

```rust
use skim::prelude::*;

fn fuzzy_select(items: Vec<String>) -> Option<String> {
    // Create skim options
    let options = SkimOptionsBuilder::default()
        .height(Some("50%"))
        .multi(false)
        .build()
        .unwrap();

    // Create a source with items
    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(std::io::Cursor::new(items.join("\n")));

    // Run skim
    let selected_items = Skim::run_with(&options, Some(items))
        .map(|out| out.selected_items)
        .unwrap_or_else(|| Vec::new());

    // Return the first selected item if any
    if !selected_items.is_empty() {
        Some(selected_items[0].output().to_string())
    } else {
        None
    }
}
```

## tmux_interface - Tmux Session Management

A Rust library for communicating with tmux via its CLI interface.

### Key Features

- Creating and managing tmux sessions
- Switching between sessions
- Checking if a session exists
- Fluid builder pattern API for tmux commands

### Basic Usage

```rust
use tmux_interface::{HasSession, KillSession, NewSession, Tmux};

fn create_session(name: &str, path: &str) -> bool {
    // Create a new detached session with the given name at the path
    let result = Tmux::with_command(
        NewSession::new()
            .detached()
            .session_name(name)
            .start_directory(path)
    ).output();

    match result {
        Ok(_) => true,
        Err(_) => false,
    }
}

fn session_exists(name: &str) -> bool {
    let status = Tmux::with_command(HasSession::new().target_session(name))
        .status()
        .map(|status| status.success())
        .unwrap_or(false);

    status
}

fn switch_to_session(name: &str) -> bool {
    // Implementation would use SwitchClient command
    true
}
```

## tracing - Structured Logging

Application-level tracing for Rust with structured, contextual logging.

### Key Features

- Span-based tracing for tracking operations
- Structured event logging with typed fields
- Hierarchical context for better debugging
- Configurable output formats and targets

### Basic Usage

```rust
use tracing::{debug, error, info, span, Level};

// Initialize the global subscriber in main
fn setup_tracing(debug_mode: bool) {
    let level = if debug_mode { Level::DEBUG } else { Level::INFO };

    tracing_subscriber::fmt()
        .with_max_level(level)
        .init();
}

// Use throughout the code
fn process_directory(path: &str) {
    // Create and enter a span for this operation
    let span = span!(Level::DEBUG, "processing_directory", path = path);
    let _enter = span.enter();

    info!("Processing directory: {}", path);

    // Any events emitted here will be associated with the span

    if path.contains("invalid") {
        error!("Invalid directory path");
        return;
    }

    debug!("Directory processed successfully");
}
```

## cross-xdg - XDG Base Directory Specification

A cross-platform implementation of the XDG Base Directory Specification, working on Linux, macOS, and Windows.

### Key Features

- Consistent access to XDG directories across all platforms
- Implements the standard XDG environment variables on Linux
- Maps to appropriate locations on macOS and Windows
- Respects environment variable overrides
- Simple API for accessing common directories

### Basic Usage

```rust
use cross_xdg::BaseDirs;
use std::path::PathBuf;

fn get_config_path(filename: &str) -> PathBuf {
    // Create a new BaseDirs instance which provides access to XDG directories
    let base_dirs = BaseDirs::new().expect("Failed to determine XDG directories");

    // Get the config directory (e.g., ~/.config on Linux)
    let config_home = base_dirs.config_home();

    // Create path for a specific configuration file
    config_home.join(filename)
}

fn get_data_path(app_name: &str, filename: &str) -> PathBuf {
    let base_dirs = BaseDirs::new().expect("Failed to determine XDG directories");

    // Get the data directory (e.g., ~/.local/share on Linux)
    let data_home = base_dirs.data_home();

    // Create application-specific subdirectory
    data_home.join(app_name).join(filename)
}
```

## Extras

### rayon - Parallel Processing (Optional Enhancement)

```rust
use rayon::prelude::*;
use walkdir::WalkDir;
use std::path::Path;

fn scan_directories_parallel(paths: &[Path]) -> Vec<PathBuf> {
    paths.par_iter()
        .flat_map(|path| {
            WalkDir::new(path)
                .max_depth(1)
                .into_iter()
                .filter_map(|e| e.ok())
                .map(|e| e.path().to_path_buf())
                .collect::<Vec<_>>()
        })
        .collect()
}
```

### thiserror - Error Handling

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TmuxSessionizerError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Git error: {0}")]
    GitError(#[from] git2::Error),

    #[error("Tmux error: {0}")]
    TmuxError(String),

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Invalid path: {0}")]
    InvalidPath(String),
}
```
