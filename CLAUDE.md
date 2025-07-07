# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Development Commands

### Building
- `cargo build` - Build in debug mode
- `cargo build --release` - Build optimized release version
- `cargo install --path .` - Install locally to `~/.cargo/bin`

### Testing
- `cargo test` - Run all tests
- `cargo test --package tmux-sessionizer` - Run tests for specific package
- `cargo test <test_name>` - Run specific test
- Note: Tests use `serial_test` crate for tests that need to run sequentially

### Linting and Quality
- `cargo fmt` - Format code
- `cargo clippy` - Run linter
- `cargo check` - Fast syntax/type checking

### Running
- `cargo run` - Run with fuzzy finder interface
- `cargo run -- <project_name>` - Direct project selection
- `cargo run -- --debug` - Run with debug logging
- `target/debug/tmux-sessionizer` - Run built binary directly

## Architecture Overview

This is a Rust CLI tool that helps manage tmux sessions by scanning directories and providing fuzzy selection. The codebase follows a modular architecture:

### Core Components

- **main.rs**: Entry point that orchestrates the entire flow
- **config.rs**: Configuration management (CLI args + TOML file)
- **directory_scanner.rs**: Recursively scans directories for projects
- **git_repository_handler.rs**: Handles Git repo and worktree detection
- **fuzzy_finder_interface.rs**: Skim-based fuzzy finder integration
- **session_manager.rs**: tmux session creation and management
- **container_detector.rs**: Detects container environments
- **path_utils.rs**: Path manipulation utilities
- **logging.rs**: Structured logging setup with tracing
- **error.rs**: Centralized error handling

### Key Dependencies

- **skim**: Fuzzy finder interface
- **tmux_interface**: tmux session management
- **git2**: Git repository operations
- **clap**: Command-line argument parsing
- **tracing**: Structured logging
- **serde/toml**: Configuration file handling
- **rayon**: Parallel processing for directory scanning

### Data Flow

1. Parse CLI args and load config from `~/.config/tmux-sessionizer/tmux-sessionizer.toml`
2. Scan configured directories in parallel using rayon
3. Filter results by exclusion patterns
4. Present options via fuzzy finder (skim) or direct selection
5. Create/switch to tmux session based on selection

### Test Structure

Tests are organized in dedicated `tests.rs` files within each module directory:
- `src/config/tests.rs`
- `src/directory_scanner/tests.rs`
- `src/git_repository_handler/tests.rs`
- etc.

Tests use `tempfile` for creating temporary directories and `serial_test` for sequential execution where needed.

## Configuration

The application uses a TOML configuration file at `~/.config/tmux-sessionizer/tmux-sessionizer.toml` with options for:
- `search_paths`: Primary directories to scan
- `additional_paths`: Extra directories to include
- `exclude_patterns`: Regex patterns for directories to exclude

See `examples/tmux-sessionizer.toml` for a complete example.

## Logging

Uses structured logging with the `tracing` crate. Logs are written to files in the configured log directory. Debug mode can be enabled via `--debug` flag or configuration.