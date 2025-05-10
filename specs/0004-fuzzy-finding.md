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

- `ai-docs/CONVENTIONS.md` (readonly)
- `ai-docs/library-reference.md` (readonly)
- `specs/0001-setup.md` (readonly)
- `Cargo.toml` (readonly)
- `README.md`
- `src/main.rs`
- `src/config.rs`
- `src/directory_scanner.rs`
- `src/git_repository_handler.rs`

### Ending context

- `src/fuzzy_finder_interface.rs` (new)
- `src/main.rs` (updated)

## Low-Level Tasks

> Ordered from start to finish

1. Create fuzzy finder module

```aider
GENERATE src/fuzzy_finder_interface.rs:
  Update fuzzy finder integration.
  Define a FuzzyFinder struct that will handle presenting options and selection.
  Implement methods to:
  - Format directory entries for display
  - Prepare the input data structure for skim
  - Return a Selected item containing path and display name
```

2. Implement fuzzy selection

```aider
UPDATE src/fuzzy_finder_interface.rs:
  Update the fuzzy selection process.
  Add a select() method that:
  - Takes a Vec<DirectoryEntry>
  - Presents them to the user via skim
  - Captures the user's selection
  - Returns the selected entry or None if cancelled
  Include proper error handling and logging.
```

3. Implement direct selection

```aider
UPDATE src/fuzzy_finder_interface.rs:
  Support direct selection.
  Add a method to:
  - Find a directory entry by path from command-line argument
  - Match partial paths if needed
  - Return the matching entry or error if not found
```

4. Integrate finder with main

```aider
UPDATE src/main.rs:
  Use the FuzzyFinder.
  Add logic to:
  - Check if a direct selection was provided in args
  - If not, use fuzzy selection
  - Handle the case where no selection is made
  - Print the final selection
```
