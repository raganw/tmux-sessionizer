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
