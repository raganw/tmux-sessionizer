# tmux-sessionizer

## Purpose

`tmux-sessionizer` is a Rust utility designed to simplify managing tmux sessions. It helps you quickly find and switch to project directories, automatically creating or attaching to tmux sessions named after those directories. This project is a Rust rewrite of an original bash script, aiming for improved performance and maintainability.

## Description

The tool scans specified search paths for project directories. It can identify Git repositories and worktrees, presenting them in a fuzzy finder interface for easy selection. Once a project is selected, `tmux-sessionizer` will either create a new tmux session for it or switch to an existing one.

## Basic Usage

Currently, the primary way to interact with the application is via command-line flags.

```bash
# Run the application (build first if necessary: cargo build)
./target/debug/tmux-sessionizer

# Enable debug logging
./target/debug/tmux-sessionizer --debug
```

Further usage instructions will be added as development progresses, including how to select projects and interact with the fuzzy finder.

## Dependencies

To use `tmux-sessionizer`, you need the following installed on your system:

- **tmux**: The terminal multiplexer itself.
- **git**: Required for Git repository and worktree detection.

## Development Status

This project is currently **in progress**. Core functionalities are being implemented.
