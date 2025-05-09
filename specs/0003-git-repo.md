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
