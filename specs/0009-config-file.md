# Specification: Configuration File Support

## High-Level Objective

- Add support for a TOML configuration file that allows users to specify search paths and exclusion patterns

## Mid-Level Objective

- Implement loading of configuration from `.config/tmux-sessionizer/tmux-sessionizer.toml`
- Define TOML schema for search paths, additional paths, and exclusion patterns
- Implement configuration merging logic (file + CLI args)
- Add validation for configuration values
- Update documentation to reflect the new configuration options

## Implementation Notes

- Use the `toml` crate for parsing TOML files
- Follow the Rust conventions from CONVENTIONS.md for file handling and error management
- Maintain backward compatibility with CLI arguments
- Implement a clear precedence order: CLI args override file config, which overrides defaults
- Ensure proper error handling for missing or malformed configuration files

## Context

### Beginning context

- `ai-docs/CONVENTIONS.md` (readonly)
- `ai-docs/library-reference.md` (readonly)
- `Cargo.toml` (readonly)
- `src/main.rs`
- `src/config.rs`
- `examples/tmux-sessionizer.toml`

### Ending context

- `examples/tmux-sessionizer.toml`

## Low-Level Tasks

> Ordered from start to finish

1. Define configuration file structure

```aider
UPDATE src/config.rs:
  Define a configuration file structure.
  Create a FileConfig struct that can be deserialized from TOML with:
  - search_paths: Option<Vec<String>>
  - additional_paths: Option<Vec<String>>
  - exclude_patterns: Option<Vec<String>>
  Include appropriate Serde attributes for proper deserialization.
```

2. Implement configuration file loading

```aider
UPDATE src/config.rs:
  Implement configuration file loading.
  Add a function to:
  - Determine the configuration file path (~/.config/tmux-sessionizer/tmux-sessionizer.toml)
  - Check if the file exists
  - Load and parse the file if it exists
  - Return a Result with the parsed configuration or an error
  Add logging for configuration file operations.
```

3. Implement configuration merging logic

```aider
UPDATE src/config.rs:
  Implement configuration merging.
  Modify the Config struct creation to:
  - Start with default values
  - Override with values from the config file if available
  - Override with CLI arguments if provided
  Ensure proper precedence: CLI args > config file > defaults
```

4. Add configuration validation

```aider
UPDATE src/config.rs:
  Add configuration validation.
  Implement validation for:
  - Path existence and accessibility
  - Pattern validity (for regex patterns)
  - Other constraints as needed
  Provide clear error messages for invalid configurations.
```

5. Update error handling for configuration

```aider
UPDATE src/error.rs:
  Enhance error handling.
  Add specific error variants for configuration file issues:
  - FileNotFound
  - ParseError
  - ValidationError
  - PermissionError
  Include context in error messages to help users fix issues.
```

6. Update main to use enhanced configuration

```aider
UPDATE src/main.rs:
  Use the enhanced configuration.
  Modify the main function to:
  - Load the configuration file
  - Merge with CLI arguments
  - Handle and report configuration errors appropriately
  - Output debug information about the final configuration
```

7. Add documentation and examples

```aider
UPDATE examples/tmux-sessionizer.toml:
  Populate with commented examples of:
  - Search paths configuration
  - Additional paths configuration
  - Exclusion pattern examples
  Include explanations of each option.
```

8. Update README with configuration file documentation

```aider
UPDATE README.md:
  Document the configuration file support.
  Add sections for:
  - Configuration file location
  - Available configuration options
  - Examples of common configurations
  - Precedence rules for configuration sources
  - Troubleshooting common configuration issues
```

9. Add unit tests for configuration file handling

```aider
UPDATE src/config.rs:
  Testing configuration file handling.
  Add tests for:
  - Loading a valid configuration file
  - Handling a missing configuration file
  - Merging configuration from different sources
  - Validating configuration values
  - Handling malformed configuration files
```
