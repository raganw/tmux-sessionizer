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
