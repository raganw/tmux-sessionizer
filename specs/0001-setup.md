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

- `ai-docs/CONVENTIONS.md` (readonly)
- `ai-docs/library-reference.md` (readonly)
- `specs/0001-setup.md` (readonly)
- `Cargo.toml` (readonly)
- `README.md`
- `src/main.rs`
- `src/config.rs`

### Ending context

- `src/main.rs`
- `src/config.rs`

## Low-Level Tasks

> Ordered from start to finish

1. Create the main module structure

```aider
UPDATE src/main.rs:
  Add basic program structure that includes module declarations.
  Set up the main function that will eventually call into our modules.
  Add tracing initialization for debug logging.
```

2. Implement basic config structure

```aider
UPDATE src/config.rs:
  Implement a Config struct that holds:
  - search_paths: Vec<PathBuf>
  - additional_paths: Vec<PathBuf>
  - exclude_patterns: Vec<Regex>
  - debug_mode: bool
  Add a default implementation that matches the bash script's defaults.
```

3. Implement command-line argument parsing

```aider
UPDATE src/config.rs to add command-line argument parsing.
Use clap to parse:
- --debug flag for enabling debug mode
- version and help information
Ensure the arguments can override the default configuration.
```

4. Connect config to main

```aider
UPDATE src/main.rs to use the Config module.
Parse command-line arguments and create a Config instance.
Add debug logging to show the loaded configuration when debug mode is enabled.
```

5. Update initial README

```aider
UPDATE README.md:
  Add basic project information.
  Include:
  - Project name and purpose
  - Brief description of what it does
  - Basic usage instructions
  - Dependencies required (tmux, git)
  - Development status (in progress)
```
