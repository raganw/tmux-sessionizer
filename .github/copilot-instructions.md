# tmux-sessionizer Development Instructions

**Always reference these instructions first and fallback to search or bash commands only when you encounter unexpected information that does not match the info here.**

## Pull Request Format

- Do not include the copilot tips section in PR descriptions
- Use semantic commit naming conventions for PR titles

## Working Effectively

Bootstrap, build, and test the repository:

- Install Rust toolchain if needed: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh && source ~/.cargo/env`
- `cargo build` -- takes ~77 seconds. **NEVER CANCEL.** Set timeout to 120+ seconds.
- `cargo build --release` -- takes ~90 seconds. **NEVER CANCEL.** Set timeout to 150+ seconds.
- `cargo install --path .` -- install to ~/.cargo/bin, takes ~90 seconds. **NEVER CANCEL.** Set timeout to 150+ seconds.
- `cargo test` -- takes ~15 seconds with 105 tests. **NEVER CANCEL.** Set timeout to 60+ seconds.
- `cargo check` -- fast syntax check, takes ~1 second.
- `cargo fmt -- --check` -- verify code formatting, takes <1 second.
- `cargo clippy -- -W clippy::pedantic -D warnings` -- linting, takes ~28 seconds. **NEVER CANCEL.** Set timeout to 90+ seconds.

**Note:** Clippy currently reports 2 collapsible-if warnings in session_manager.rs. These are pre-existing and not related to new changes.

## Running the Application

**Prerequisites:** Ensure `tmux` and `git` are installed on the system.

- Run with fuzzy finder interface: `cargo run` or `./target/debug/tmux-sessionizer`
- Run with direct project selection: `cargo run -- project_name` or `./target/debug/tmux-sessionizer project_name`
- Run with debug logging: `cargo run -- --debug` or `./target/debug/tmux-sessionizer --debug`
- Initialize configuration: `cargo run -- --init` or `./target/debug/tmux-sessionizer --init`

## Configuration

The application uses a TOML configuration file at `~/.config/tmux-sessionizer/tmux-sessionizer.toml`. 

Key configuration options:
- `search_paths`: Primary directories to scan for projects
- `additional_paths`: Extra directories to include
- `exclude_patterns`: Regex patterns to exclude directories

Use `--init` flag to create a template configuration file with examples.

## Validation Scenarios

**Always manually validate changes with these complete end-to-end scenarios:**

### Scenario 1: Basic Session Creation
```bash
# Create test project structure
mkdir -p /tmp/test-validation/{project-a,project-b}
echo "# Test A" > /tmp/test-validation/project-a/README.md

# Create test config
mkdir -p ~/.config/tmux-sessionizer
cat > ~/.config/tmux-sessionizer/tmux-sessionizer.toml << 'EOF'
search_paths = ["/tmp/test-validation"]
EOF

# Test direct selection (should create tmux session)
./target/debug/tmux-sessionizer project-a

# Verify: Should switch to /tmp/test-validation/project-a directory in new tmux session
# Exit tmux with: exit
```

### Scenario 2: Git Repository Detection
```bash
# Create git repository in test area
cd /tmp/test-validation
git init git-repo-test
cd git-repo-test
git config user.email "test@example.com" && git config user.name "Test User"
echo "# Git Test" > README.md
git add README.md && git commit -m "Initial commit"

# Test git repository detection with debug logging
./target/debug/tmux-sessionizer --debug git-repo-test

# Verify: Should detect as GitRepository type and create session
```

### Scenario 3: Configuration Initialization
```bash
# Test in clean environment
rm -rf ~/.config/tmux-sessionizer
./target/debug/tmux-sessionizer --init

# Verify: Should create ~/.config/tmux-sessionizer/tmux-sessionizer.toml with template content
cat ~/.config/tmux-sessionizer/tmux-sessionizer.toml
```

**Critical:** Run at least one complete validation scenario after any changes to ensure the application remains fully functional.

## Build Timing Expectations

- **Debug build**: ~77 seconds - **NEVER CANCEL, SET TIMEOUT TO 120+ SECONDS**
- **Release build**: ~90 seconds - **NEVER CANCEL, SET TIMEOUT TO 150+ SECONDS**
- **Test suite**: ~15 seconds (105 tests) - **NEVER CANCEL, SET TIMEOUT TO 60+ SECONDS**
- **Clippy linting**: ~28 seconds - **NEVER CANCEL, SET TIMEOUT TO 90+ SECONDS**
- **Code formatting check**: <1 second
- **Quick syntax check**: ~1 second

## Pre-commit Validation

Always run these commands before committing changes:

1. `cargo test` -- ensure all tests pass
2. `cargo fmt -- --check` -- verify formatting (or run `cargo fmt` to fix)
3. `cargo clippy -- -W clippy::pedantic -D warnings` -- check for linter warnings

**Note**: The codebase currently has 2 collapsible-if clippy warnings in `src/session_manager.rs` lines 112 and 154. These are existing and not caused by your changes. Clippy will fail with these warnings when running with `-D warnings` flag. To run clippy without failing on these warnings, use: `cargo clippy -- -W clippy::pedantic -A clippy::collapsible-if`

## Architecture Overview

This Rust CLI tool manages tmux sessions with these key components:

- **main.rs**: Application entry point and orchestration
- **config/**: Configuration management (CLI args + TOML file)
- **directory_scanner/**: Parallel directory scanning with rayon
- **git_repository_handler/**: Git repository and worktree detection
- **fuzzy_finder_interface/**: Skim-based fuzzy selection
- **session_manager/**: tmux session creation and management
- **logging/**: Structured logging with tracing crate

### Key Dependencies
- **skim**: Fuzzy finder interface
- **tmux_interface**: tmux session operations
- **git2**: Git repository handling
- **rayon**: Parallel directory processing
- **tracing**: Structured logging
- **clap**: CLI argument parsing

## Test Structure

Tests are modular with dedicated `tests.rs` files in each component directory:
- Uses `serial_test` crate for sequential execution where needed
- Uses `tempfile` for temporary directory creation
- Comprehensive test coverage including git repository scenarios

Run specific test modules:
- `cargo test --package tmux-sessionizer` -- all tests
- `cargo test config::tests` -- config module tests
- `cargo test directory_scanner::tests` -- scanner tests

## Common File Locations

Repository structure:
```
/
├── src/
│   ├── main.rs                    # Application entry point
│   ├── config/                    # Configuration handling
│   ├── directory_scanner/         # Directory scanning logic
│   ├── git_repository_handler/    # Git repository detection
│   ├── fuzzy_finder_interface/    # Fuzzy finder integration
│   ├── session_manager/          # tmux session management
│   └── ...
├── examples/tmux-sessionizer.toml # Example configuration
├── CLAUDE.md                     # Development guidance
└── README.md                     # User documentation
```

Key files to check after changes:
- Always run validation scenarios after modifying core logic
- Check `src/main.rs` for application flow changes
- Review `src/config/` for configuration handling
- Test `src/session_manager/` for tmux integration changes
- Validate `src/directory_scanner/` for project discovery changes

## CI/CD Information

The GitHub Actions workflow (`.github/workflows/ci.yml`) runs on:
- Linux (ubuntu-latest)
- macOS Intel (macos-13) 
- macOS ARM (macos-14)

Workflow steps match the validation commands above:
1. `cargo build --verbose`
2. `cargo test --verbose` 
3. `cargo clippy -- -W clippy::pedantic -D warnings`
4. `cargo fmt -- --check`

All platforms require OpenSSL for git2 dependency compilation.

## Error Handling

The application has centralized error handling in `src/error.rs` using the `anyhow` and `thiserror` crates. When adding new error conditions:

1. Add appropriate error variants to existing error enums
2. Use `?` operator for error propagation
3. Add context with `.context()` for better error messages
4. Ensure errors are logged with appropriate levels

## Performance Considerations

- Directory scanning uses `rayon` for parallel processing
- Git operations use `git2` crate for efficiency
- Test suite uses `serial_test` only where necessary for isolation
- Release builds are optimized and significantly faster than debug builds

Always test performance-critical changes with both debug and release builds.
