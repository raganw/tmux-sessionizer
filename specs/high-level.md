# High-Level Specification for Rust Rewrite of Project Selector

## Overview

This specification outlines a Rust rewrite of a bash script that searches directories, processes Git repositories/worktrees, and creates/switches to tmux sessions based on user selection using a fuzzy finder interface.

## Core Libraries

1. **walkdir** (or **ignore**): For efficient directory traversal
2. **git2-rs**: For Git repository operations
3. **skim** (or **nucleo**): For the fuzzy finder interface
4. **tmux_interface**: For tmux session management
5. **clap**: For command-line argument parsing
6. **tracing**: For structured logging (replaces debug mode)

## Application Structure

### 1. Configuration Module

- Define configuration structures for search paths, exclusion patterns
- Support command-line arguments (including debug mode)
- Load default configuration and allow overrides

### 2. Directory Scanner Module

- Traverse the file system based on configured paths
- Apply exclusion filters
- Flag Git repositories for special processing
- Return structured data representing discovered directories

### 3. Git Repository Handler Module

- Process Git repositories to identify worktrees
- Determine main worktree for a repository group
- Handle linked worktrees and their relationships
- Create properly formatted display names for repositories and worktrees

### 4. Session Manager Module

- Map scanned directories to potential tmux session names
- Check existing tmux sessions
- Create new sessions or switch to existing ones
- Sanitize directory names for valid tmux session names

### 5. Fuzzy Finder Interface Module

- Prepare input data for the fuzzy finder
- Format display entries with appropriate delimiters
- Handle user selection
- Extract selected path and display name

### 6. Main Application Logic

- Initialize configuration
- Scan directories
- Process Git repositories
- Present fuzzy finder interface
- Handle selection (direct or via fuzzy finder)
- Manage tmux sessions

## Data Flow

1. Parse command-line arguments
2. Initialize configuration (search paths, exclusion patterns)
3. Scan directories according to configuration
4. Process Git repositories to identify worktrees and relationships
5. Format directories and repositories for display
6. Present fuzzy finder interface (unless direct selection provided)
7. Process selection to determine actual path and session name
8. Create or switch to tmux session

## Data Structures

### Config

```rust
struct Config {
    search_paths: Vec<PathBuf>,
    additional_paths: Vec<PathBuf>,
    exclude_patterns: Vec<Regex>,
    debug_mode: bool,
}
```

### Directory Entry

```rust
enum DirectoryType {
    Plain,
    GitRepository,
    GitWorktree { main_worktree: PathBuf },
    GitWorktreeContainer,
}

struct DirectoryEntry {
    path: PathBuf,
    display_name: String,
    entry_type: DirectoryType,
    parent_path: Option<PathBuf>,  // For worktrees to reference their main repo
}
```

### Selection

```rust
struct Selection {
    path: PathBuf,
    display_name: String,
    session_name: String,
}
```

## Key Functionalities

### 1. Directory Scanning

- Use `walkdir` to traverse directories with depth constraints
- Filter directories based on exclusion patterns
- Identify Git repositories using `git2-rs`

### 2. Git Repository Processing

- Detect Git repositories and their worktrees
- Determine the relationship between worktrees
- Generate formatted display names (with parent repo if applicable)

### 3. Fuzzy Selection

- Format entries for display in fuzzy finder
- Use `skim` to present selection interface
- Parse selection to extract path and display name

### 4. Tmux Integration

- Check if tmux is running
- Format session name based on selection
- Create new session or switch to existing session

### 5. Error Handling and Logging

- Implement robust error handling throughout
- Use tracing for structured logging based on debug mode

## Performance Considerations

- Process directories in parallel where appropriate
- Avoid unnecessary Git operations
- Cache repository information when processing multiple paths

## User Experience

- Maintain familiar interface from the bash script
- Improve error messages and handling
- Add more descriptive debug output when enabled

## Future Enhancements

- Configuration file support
- Custom key bindings for the fuzzy finder
- Additional view options for directory listing
- Preview pane showing directory contents

