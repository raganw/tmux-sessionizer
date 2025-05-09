# Tmux Sessionizer: Rust Implementation Blueprint

After analyzing the provided files, I've created a detailed blueprint for rewriting the `tmux-sessionizer` bash script in Rust. This document outlines the step-by-step approach, breaking down the project into manageable chunks that build upon each other.

## Project Overview

The goal is to rewrite the `tmux-sessionizer` bash script into Rust. This tool simplifies managing tmux sessions by:

1. Scanning directories for potential projects
2. Handling Git repositories and worktrees with special formatting
3. Presenting options via fuzzy finder
4. Creating or switching to tmux sessions

## Core Modules

Based on the high-level specification, the Rust implementation will consist of these core modules:

1. **Configuration Module**: Handles configuration loading and CLI args
2. **Directory Scanner Module**: Traverses the file system and applies filters
3. **Git Repository Handler Module**: Processes Git repositories and worktrees
4. **Session Manager Module**: Manages tmux sessions
5. **Fuzzy Finder Interface Module**: Integrates with a fuzzy finder
6. **Main Application Logic**: Coordinates all the modules

## Implementation Steps

I've broken down the implementation into a series of small, incremental steps that build upon each other. Each step is designed to be testable and provides a foundation for the next step.

### 1. Project Setup and Basic Configuration

This first specification sets up the project structure and implements the basic configuration module.

# Specification 1: Project Setup and Basic Configuration

> Ingest the information from this file, implement the Low-Level Tasks, and generate the code that will satisfy the High and Mid-Level Objectives.

## High-Level Objective

- Set up the Rust project structure and implement basic configuration handling

## Mid-Level Objective

- Create a new Rust project with the appropriate structure
- Define configuration structures for storing search paths and exclusion patterns
- Implement basic command-line argument parsing
- Add debug mode flag similar to the bash script

## Implementation Notes

- Use `clap` for command-line argument parsing
- Follow the Rust conventions from CONVENTIONS.md for project structure and naming
- Default configuration should match the behavior of the original bash script
- Use `tracing` instead of the bash script's debug logging function

## Context

### Beginning context

- (No files exist yet, this is the initial setup)

### Ending context

- `Cargo.toml`
- `src/main.rs`
- `src/config.rs`
- `README.md`

## Low-Level Tasks

> Ordered from start to finish

1. Initialize a new Rust project with Cargo

```aider
CREATE a new Rust project named "tmux-sessionizer" with appropriate dependencies.
Use Cargo.toml to set up the project with the following dependencies:
- clap (for command-line parsing)
- walkdir (for directory traversal)
- regex (for pattern matching)
- tracing (for structured logging)
- git2 (for Git repository operations)
Please include appropriate version numbers for each dependency.
```

2. Create the main module structure

```aider
CREATE src/main.rs with a basic program structure that includes module declarations.
Set up the main function that will eventually call into our modules.
Add tracing initialization for debug logging.
```

3. Implement basic config structure

```aider
CREATE src/config.rs module.
Implement a Config struct that holds:
- search_paths: Vec<PathBuf>
- additional_paths: Vec<PathBuf>
- exclude_patterns: Vec<Regex>
- debug_mode: bool
Add a default implementation that matches the bash script's defaults.
```

4. Implement command-line argument parsing

```aider
UPDATE src/config.rs to add command-line argument parsing.
Use clap to parse:
- --debug flag for enabling debug mode
- version and help information
Ensure the arguments can override the default configuration.
```

5. Connect config to main

```aider
UPDATE src/main.rs to use the Config module.
Parse command-line arguments and create a Config instance.
Add debug logging to show the loaded configuration when debug mode is enabled.
```

6. Create initial README

```aider
CREATE README.md with basic project information.
Include:
- Project name and purpose
- Brief description of what it does
- Basic usage instructions
- Dependencies required (tmux, git)
- Development status (in progress)
```

# Specification 2: Directory Scanner Module

> Ingest the information from this file, implement the Low-Level Tasks, and generate the code that will satisfy the High and Mid-Level Objectives.

## High-Level Objective

- Implement the directory scanner module that traverses file systems and identifies potential project directories

## Mid-Level Objective

- Create a DirectoryEntry data structure to represent discovered directories
- Implement directory traversal based on configured paths
- Apply exclusion filters to skip unwanted directories
- Return a structured list of discovered directories

## Implementation Notes

- Use `walkdir` for efficient directory traversal
- Implement proper path resolution similar to the bash script (using canonical paths)
- Handle symbolic links properly
- Ensure exclusion filters work on both original and resolved paths
- For now, just identify directories as plain (Git handling will come later)

## Context

### Beginning context

- `Cargo.toml` (from previous spec)
- `src/main.rs` (from previous spec)
- `src/config.rs` (from previous spec)
- `README.md` (from previous spec)

### Ending context

- `src/scanner.rs` (new)
- `src/main.rs` (updated)

## Low-Level Tasks

> Ordered from start to finish

1. Create the directory entry structure

```aider
CREATE src/scanner.rs module.
Define a DirectoryEntry struct to represent discovered directories with:
- path: PathBuf (original path)
- resolved_path: PathBuf (canonical path)
- display_name: String (for presentation to user)
For now, implement a simple DirectoryType enum with just 'Plain' variant.
Include appropriate Debug/Clone/PartialEq implementations.
```

2. Implement basic directory scanner

```aider
UPDATE src/scanner.rs to implement directory scanning functionality.
Create a DirectoryScanner struct that uses the Config.
Implement a scan() method that:
- Traverses directories in search_paths with depth=1
- Follows symlinks properly
- Resolves paths to canonical form
- Applies exclude_patterns to both original and canonical paths
- Returns a Vec<DirectoryEntry>
```

3. Process additional paths

```aider
UPDATE src/scanner.rs to handle additional_paths from Config.
Extend the scan() method to:
- Process additional_paths separately from search_paths
- Apply the same filtering and resolution logic
- Prevent duplicates based on canonical paths
- Add these to the returned Vec<DirectoryEntry>
```

4. Add logging for scanner operations

```aider
UPDATE src/scanner.rs to add detailed logging.
Add tracing debug logs for:
- Start of scanning process
- Directories found in initial search
- Path resolution results
- Exclusion filter applications
- Final scan results
```

5. Integrate scanner with main

```aider
UPDATE src/main.rs to use the DirectoryScanner.
Create a scanner instance with the config.
Call scan() and print the results (for now).
Ensure debug output is controlled by the config.debug_mode flag.
```

# Specification 3: Git Repository Detection and Processing

> Ingest the information from this file, implement the Low-Level Tasks, and generate the code that will satisfy the High and Mid-Level Objectives.

## High-Level Objective

- Enhance the directory scanner to detect and process Git repositories and worktrees

## Mid-Level Objective

- Extend the DirectoryType enum to include Git repositories and worktrees
- Detect Git repositories during directory scanning
- Identify worktrees within Git repositories
- Determine the relationships between worktrees
- Generate properly formatted display names for repositories and worktrees

## Implementation Notes

- Use `git2` crate for Git repository detection
- Consider implementing a separate Git-specific module for repository processing
- Handle worktree relationships similar to the bash script
- Ensure display names match the format from the bash script: `[parent] worktree_name`
- Implement the container logic where appropriate

## Context

### Beginning context

- `Cargo.toml`
- `src/main.rs`
- `src/config.rs`
- `src/scanner.rs`

### Ending context

- `src/git.rs` (new)
- `src/scanner.rs` (updated)
- `src/main.rs` (updated)

## Low-Level Tasks

> Ordered from start to finish

1. Extend DirectoryType enum

```aider
UPDATE src/scanner.rs to extend the DirectoryType enum.
Add variants for:
- GitRepository (standard git repo)
- GitWorktree with a field for main_worktree path
- GitWorktreeContainer (directory containing worktrees)
Update the DirectoryEntry struct to include parent_path for worktrees.
```

2. Create Git repository handler

```aider
CREATE src/git.rs module for Git-specific operations.
Implement functions to:
- Detect if a path is a Git repository
- Get the Git directory (.git) path for a repository
- Check if a directory is a bare repository
Include proper error handling and logging.
```

3. Implement worktree listing and processing

```aider
UPDATE src/git.rs to implement worktree operations.
Add functions to:
- List all worktrees for a Git repository
- Determine the main worktree of a repository
- Parse worktree information from git output
- Establish relationships between worktrees
Include detailed logging for debugging.
```

4. Integrate Git processing with scanner

```aider
UPDATE src/scanner.rs to use the git module.
Modify the scan() method to:
- Detect Git repositories during scanning
- Process Git worktrees when found
- Set the correct DirectoryType for each entry
- Handle the container logic for directories containing worktrees
```

5. Update display name formatting

```aider
UPDATE src/scanner.rs to implement display name formatting.
Update code to format display names:
- For plain directories: basename
- For Git repositories: basename
- For Git worktrees: [parent_basename] worktree_basename
- Skip adding container directories directly when their children are added
```

6. Update main to show Git information

```aider
UPDATE src/main.rs to display Git repository information.
When printing the scan results, show:
- Directory type (plain, git, worktree)
- Display name with proper formatting
- Parent repository information for worktrees
```

# Specification 4: Fuzzy Finder Integration

> Ingest the information from this file, implement the Low-Level Tasks, and generate the code that will satisfy the High and Mid-Level Objectives.

## High-Level Objective

- Implement the fuzzy finder interface for directory selection

## Mid-Level Objective

- Format directory entries for display in the fuzzy finder
- Integrate with a fuzzy finder library (skim) to present options to the user
- Handle user selection from the fuzzy finder
- Support direct selection from command-line arguments
- Extract the selected path and display name

## Implementation Notes

- Use the `skim` crate as a Rust alternative to fzf
- Format entries with display name and path similar to the bash script
- Support direct selection from command-line arguments like the bash script
- Handle the case where no selection is made
- Ensure error handling is robust

## Context

### Beginning context

- `Cargo.toml`
- `src/main.rs`
- `src/config.rs`
- `src/scanner.rs`
- `src/git.rs`

### Ending context

- `src/finder.rs` (new)
- `src/main.rs` (updated)
- `Cargo.toml` (updated with skim dependency)

## Low-Level Tasks

> Ordered from start to finish

1. Add skim dependency

```aider
UPDATE Cargo.toml to add the skim dependency.
Add skim with an appropriate version number.
```

2. Create fuzzy finder module

```aider
CREATE src/finder.rs module for fuzzy finder integration.
Define a FuzzyFinder struct that will handle presenting options and selection.
Implement methods to:
- Format directory entries for display
- Prepare the input data structure for skim
- Return a Selected item containing path and display name
```

3. Implement fuzzy selection

```aider
UPDATE src/finder.rs to implement the fuzzy selection process.
Add a select() method that:
- Takes a Vec<DirectoryEntry>
- Presents them to the user via skim
- Captures the user's selection
- Returns the selected entry or None if cancelled
Include proper error handling and logging.
```

4. Implement direct selection

```aider
UPDATE src/finder.rs to support direct selection.
Add a method to:
- Find a directory entry by path from command-line argument
- Match partial paths if needed
- Return the matching entry or error if not found
```

5. Integrate finder with main

```aider
UPDATE src/main.rs to use the FuzzyFinder.
Add logic to:
- Check if a direct selection was provided in args
- If not, use fuzzy selection
- Handle the case where no selection is made
- Print the final selection
```

# Specification 5: Tmux Session Management

> Ingest the information from this file, implement the Low-Level Tasks, and generate the code that will satisfy the High and Mid-Level Objectives.

## High-Level Objective

- Implement tmux session management functionality

## Mid-Level Objective

- Generate valid tmux session names from directory paths
- Check if tmux is running and if specific sessions exist
- Create new tmux sessions or switch to existing ones
- Implement the same session behavior as the bash script

## Implementation Notes

- Use proper error handling for tmux commands
- Sanitize session names similarly to the bash script
- Handle the different cases: no tmux running, tmux running but session doesn't exist, session exists
- Prefer using the tmux_interface crate for tmux operations
- Follow the same session naming convention as the bash script

## Context

### Beginning context

- `Cargo.toml`
- `src/main.rs`
- `src/config.rs`
- `src/scanner.rs`
- `src/git.rs`
- `src/finder.rs`

### Ending context

- `src/session.rs` (new)
- `src/main.rs` (updated)
- `Cargo.toml` (updated with tmux_interface dependency)

## Low-Level Tasks

> Ordered from start to finish

1. Add tmux_interface dependency

```aider
UPDATE Cargo.toml to add the tmux_interface dependency.
Add tmux_interface with an appropriate version number.
```

2. Create session manager module

```aider
CREATE src/session.rs module for tmux session management.
Define a SessionManager struct that will handle tmux operations.
Implement methods to:
- Generate sanitized session names from paths
- Special handling for worktree naming format: [parent] worktree
- Check if a tmux server is running
- Check if a specific session exists
```

3. Implement session creation and switching

```aider
UPDATE src/session.rs to implement session management.
Add methods to:
- Create a new tmux session with a given name and directory
- Switch to an existing session
- Handle the case where tmux is not running
- Handle appropriate error conditions
```

4. Implement Selection struct and logic

```aider
UPDATE src/session.rs to define a Selection struct.
The Selection should include:
- path: PathBuf
- display_name: String
- session_name: String
Add methods to create a Selection from a DirectoryEntry.
```

5. Integrate session manager with main

```aider
UPDATE src/main.rs to use the SessionManager.
Update the main function to:
- Create a Selection from the chosen DirectoryEntry
- Initialize the SessionManager
- Create or switch to the appropriate tmux session
- Handle errors properly
```

# Specification 6: Final Integration and Error Handling

> Ingest the information from this file, implement the Low-Level Tasks, and generate the code that will satisfy the High and Mid-Level Objectives.

## High-Level Objective

- Complete the integration of all modules and implement robust error handling

## Mid-Level Objective

- Wire up all modules in the main application
- Implement comprehensive error handling throughout the application
- Add detailed logging with different verbosity levels
- Ensure the application behavior matches the original bash script
- Test the end-to-end workflow

## Implementation Notes

- Create a proper error handling strategy using thiserror
- Ensure all errors are propagated properly
- Make error messages user-friendly
- Use tracing for structured logging with different levels
- Consider adding a Result type alias for common error handling

## Context

### Beginning context

- `Cargo.toml`
- `src/main.rs`
- `src/config.rs`
- `src/scanner.rs`
- `src/git.rs`
- `src/finder.rs`
- `src/session.rs`

### Ending context

- `src/error.rs` (new)
- All other files updated with error handling
- `README.md` (updated with final usage instructions)

## Low-Level Tasks

> Ordered from start to finish

1. Create error module

```aider
CREATE src/error.rs module for centralized error handling.
Define an Error enum using thiserror with variants for:
- ConfigError
- ScannerError
- GitError
- FinderError
- SessionError
- TmuxError
- IOError (wrapping std::io::Error)
Add appropriate Display implementations.
Create a Result type alias for the application.
```

2. Update all modules to use the error type

```aider
UPDATE all modules to use the new error type.
Replace individual error handling with the centralized Error type.
Use proper error propagation with the ? operator.
Ensure error messages are clear and helpful.
```

3. Enhance logging throughout the application

```aider
UPDATE all modules to improve logging.
Add structured logging with different levels:
- error: for critical errors
- warn: for non-critical issues
- info: for normal operation information
- debug: for detailed debugging information
- trace: for very verbose operation details
Ensure debug logging only appears when debug_mode is true.
```

4. Complete the main application logic

```aider
UPDATE src/main.rs to finalize the application logic.
Implement the complete flow:
1. Parse config
2. Scan directories
3. Select directory (direct or fuzzy)
4. Create/switch tmux session
Ensure proper error handling throughout.
```

5. Update README with final instructions

```aider
UPDATE README.md with comprehensive usage instructions.
Include:
- Installation instructions
- All command-line options
- Examples of common use cases
- Requirements (tmux, git)
- Configuration options
```

# Specification 7: Performance Optimization and Testing

> Ingest the information from this file, implement the Low-Level Tasks, and generate the code that will satisfy the High and Mid-Level Objectives.

## High-Level Objective

- Optimize performance and add testing to ensure reliability

## Mid-Level Objective

- Implement parallel directory scanning where appropriate
- Add caching for Git repository information
- Add unit tests for each module
- Add integration tests for the complete workflow
- Ensure test coverage for edge cases

## Implementation Notes

- Use Rayon for parallel processing
- Consider using a simple in-memory cache for Git repository information
- Follow the testing conventions from CONVENTIONS.md
- Use appropriate mocking for external dependencies
- Ensure tests run in isolation

## Context

### Beginning context

- All files from previous specifications

### Ending context

- `tests/` directory with test files
- Updated modules with performance optimizations
- `benches/` directory with benchmarks

## Low-Level Tasks

> Ordered from start to finish

1. Add parallel processing for directory scanning

```aider
UPDATE src/scanner.rs to implement parallel directory scanning.
Add the rayon dependency to Cargo.toml.
Use parallel iterators where appropriate.
Ensure thread safety throughout.
```

2. Implement Git repository caching

```aider
UPDATE src/git.rs to add caching.
Implement a simple cache for:
- Repository detection results
- Worktree listing results
Use a thread-safe cache structure.
Add cache invalidation where appropriate.
```

3. Add unit tests for config module

```aider
CREATE tests/config_tests.rs for testing the config module.
Add tests for:
- Default configuration
- Command-line argument parsing
- Configuration merging
Include edge cases like invalid paths.
```

4. Add unit tests for scanner module

```aider
CREATE tests/scanner_tests.rs for testing the scanner module.
Add tests for:
- Basic directory scanning
- Exclude pattern matching
- Git repository detection
- Worktree processing
Include test fixtures where necessary.
```

5. Add unit tests for other modules

```aider
CREATE tests for the remaining modules:
- git_tests.rs
- finder_tests.rs
- session_tests.rs
- error_tests.rs
Include appropriate test cases for each module.
```

6. Add integration tests

```aider
CREATE tests/integration_test.rs for end-to-end testing.
Test the complete workflow:
1. Configuration loading
2. Directory scanning
3. Selection
4. Session management
Mock external dependencies where necessary.
```

7. Add benchmarks

```aider
CREATE benches/benchmark.rs for performance benchmarking.
Add benchmarks for:
- Directory scanning
- Git repository processing
- Fuzzy finding
Use criterion for benchmarking.
```

# Specification 8: Documentation and Polish

> Ingest the information from this file, implement the Low-Level Tasks, and generate the code that will satisfy the High and Mid-Level Objectives.

## High-Level Objective

- Complete the documentation and add final polish to the application

## Mid-Level Objective

- Add comprehensive documentation to all modules
- Create user-friendly help and usage messages
- Add examples and documentation tests
- Ensure code follows Rust best practices
- Add CI configuration

## Implementation Notes

- Follow the documentation conventions from CONVENTIONS.md
- Use doc comments (`///`) for all public items
- Include examples in documentation
- Use appropriate Markdown formatting
- Consider adding a man page

## Context

### Beginning context

- All files from previous specifications

### Ending context

- All files updated with documentation
- `.github/` directory with CI configuration
- `examples/` directory with example usage

## Low-Level Tasks

> Ordered from start to finish

1. Add documentation to all modules

```aider
UPDATE all source files to add comprehensive documentation.
Add doc comments (`///`) to:
- Module-level documentation
- Struct and enum documentation
- Function and method documentation
Include examples where appropriate.
```

2. Improve help and usage messages

```aider
UPDATE src/config.rs to improve CLI help messages.
Enhance clap configuration to:
- Provide detailed help for each option
- Add examples in the help text
- Show version information
- Group related options
```

3. Add documentation tests

```aider
UPDATE source files to include documentation tests.
Add examples that serve as both documentation and tests.
Ensure examples are runnable and correct.
```

4. Create examples

```aider
CREATE examples/ directory with example usage.
Add example scripts showing:
- Basic usage
- Advanced configuration
- Integration with other tools
```

5. Add CI configuration

```aider
CREATE .github/workflows/ci.yml for continuous integration.
Configure GitHub Actions to:
- Build the project
- Run tests
- Run lints (clippy)
- Check formatting (rustfmt)
- Run on multiple platforms
```

6. Final code review and cleanup

```aider
REVIEW all code to ensure it follows best practices.
Check for:
- Proper error handling
- Consistent naming conventions
- Code organization
- Unnecessary dependencies
- Performance issues
Make necessary adjustments.
```

## Final Blueprint Summary

This implementation plan provides a comprehensive, step-by-step approach to rewriting the tmux-sessionizer bash script in Rust. Each specification builds on the previous ones, with a focus on:

1. **Incremental development**: Starting with basic functionality and building more complex features gradually
2. **Test-driven approach**: Adding tests throughout the process
3. **Good practices**: Following Rust conventions and best practices
4. **Clear documentation**: Ensuring the code is well-documented

The specifications have been broken down into manageable chunks that can be implemented one by one, with clear objectives and detailed tasks for each step. This approach minimizes risk and allows for regular validation of progress.
