# Specification: Logging Module Implementation

## High-Level Objective

- Move tracing setup to a dedicated module with file-based logging and rotation

## Mid-Level Objective

- Create a new `logging.rs` module to handle all tracing/logging concerns
- Configure logging to write exclusively to a log file in the standard XDG data directory
- Implement daily log rotation keeping only the most recent 2 files
- Remove all logging to stderr/stdout
- Add unit tests for the logging module

## Implementation Notes

- Use `cross-xdg` to locate the appropriate XDG data directory
- Use `tracing` and `tracing-appender` for logging and file rotation
- Follow the Rust conventions from CONVENTIONS.md for naming and structure
- Ensure logging initialization happens early in the application lifecycle
- Make debug level toggleable via the config.debug_mode flag

## Context

### Beginning context

- `ai-docs/CONVENTIONS.md` (readonly)
- `ai-docs/library-reference.md` (readonly)
- `Cargo.toml` (readonly)
- `src/main.rs`

### Ending context

- `src/logging.rs`

## Low-Level Tasks

> Ordered from start to finish

1. Create basic logging module

```aider
UPDATE src/logging.rs:
  Use appropriate imports for tracing and cross-xdg.
  Create a Logger struct that will encapsulate the logging functionality.
  Add documentation comments following the conventions in CONVENTIONS.md.
```

2. Implement XDG directory detection

```aider
UPDATE src/logging.rs:
  Implement XDG directory detection.
  Use cross-xdg to find the appropriate data directory for the application.
  Create the directory if it doesn't exist.
  Build the log file path using the application name.
  Add proper error handling for file operations.
```

3. Implement log file rotation

```aider
UPDATE src/logging.rs:
  Implement log file rotation.
  Use tracing-appender to create a rolling file appender.
  Configure for daily rotation.
  Set up to keep only the last 2 log files.
  Ensure the rotation logic handles file creation/deletion correctly.
```

4. Implement logger initialization

```aider
UPDATE src/logging.rs:
  Implement the init function.
  Create a public init function that:
  - Takes a reference to Config to check debug_mode
  - Sets up the tracing subscriber with the file appender
  - Configures the appropriate filter based on debug_mode (debug vs info)
  - Returns a guard that must be kept alive for logging to work
  Add detailed documentation for this function.
```

5. Update main to use the new logging module

```aider
UPDATE src/main.rs:
  Use the new logging module.
  Remove the existing setup_tracing function.
  Call the logging module's init function early in main.
  Store the returned guard in a variable that lives for the duration of the program.
  Update imports accordingly.
```

6. Add unit tests for the logging module

```aider
UPDATE src/logging.rs:
  Add unit tests.
  Add tests at the end of the file in a #[cfg(test)] module.
  Test cases should include:
  - Test that the XDG directory is correctly determined
  - Test that log files are created in the expected location
  - Test that rotation correctly keeps only 2 files
  - Test that debug mode correctly changes the filter level
  Use appropriate mocking for file system operations where needed.
```

7. Final review and cleanup

```aider
UPDATE src/logging.rs:
  Make sure all functions have comprehensive documentation.
  Ensure best practices like proper error handling are followed.
  Clean up any unnecessary debug prints or commented code.
  Ensure the code follows all conventions from CONVENTIONS.md.
```
