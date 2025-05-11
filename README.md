# tmux-sessionizer

## Purpose

`tmux-sessionizer` is a Rust utility designed to simplify managing tmux sessions. It helps you quickly find and switch to project directories, automatically creating or attaching to tmux sessions named after those directories. This project is a Rust rewrite of an original bash script, aiming for improved performance and maintainability.

## Description

The tool scans specified search paths for project directories. It can identify Git repositories and worktrees, presenting them in a fuzzy finder interface (powered by Skim) for easy selection. Once a project is selected, `tmux-sessionizer` will either create a new tmux session for it or switch/attach to an existing one. Session names are intelligently derived from the project directory or Git worktree structure.

## Features

*   **Fuzzy Project Selection**: Quickly find projects using an interactive fuzzy finder.
*   **Git Integration**: Automatically detects Git repositories and worktrees, providing enhanced display names (e.g., `repo_name (worktree_name)`).
*   **Smart Session Naming**: Generates clean and descriptive tmux session names.
*   **Automatic Session Management**: Creates new tmux sessions or attaches to existing ones.
*   **Direct Selection**: Optionally bypass the fuzzy finder by providing a project name or path directly.
*   **Configurable Search Paths**: Scans predefined common development directories (currently `~/Development`, `~/Development/raganw`, `~/.config`). (Future: customizable via config file).

## Requirements

To build and use `tmux-sessionizer`, you need the following installed on your system:

*   **Rust toolchain**: (e.g., via [rustup](https://rustup.rs/)) for building the application.
*   **tmux**: The terminal multiplexer itself. The application interacts with `tmux` to manage sessions.
*   **git**: Required for Git repository and worktree detection features.

## Installation

### From Source

1.  **Clone the repository (if you haven't already):**
    ```bash
    git clone <repository_url> # Replace <repository_url> with the actual URL
    cd tmux-sessionizer
    ```
2.  **Build the project:**
    ```bash
    cargo build
    ```
    The executable will be located at `./target/debug/tmux-sessionizer`. For a release build, use `cargo build --release` (executable at `./target/release/tmux-sessionizer`).

3.  **Install the binary (optional):**
    To make it available in your PATH, you can install it using:
    ```bash
    cargo install --path .
    ```
    This will typically place the binary in `~/.cargo/bin/tmux-sessionizer`. Ensure `~/.cargo/bin` is in your `PATH`.

## Usage

### Synopsis
```bash
tmux-sessionizer [OPTIONS] [DIRECT_SELECTION]
```

### Command-Line Options

*   `[DIRECT_SELECTION]` (Positional Argument)
    *   Specifies a direct path or name to select.
    *   If provided, the fuzzy finder interface is skipped.
    *   The application will attempt to find a unique match among the scanned project directories based on this argument. If a unique match is found, it will be selected. If multiple matches are found or no match is found, an appropriate message will be displayed.
    *   Example: `tmux-sessionizer my_project` or `tmux-sessionizer ~/Development/another_project`

*   `-d, --debug`
    *   Enables detailed debug logging output.
    *   Useful for troubleshooting or understanding the application's behavior.

### Examples

1.  **Launch with fuzzy finder:**
    Simply run the command without any arguments to open the fuzzy finder interface, listing all discovered projects.
    ```bash
    tmux-sessionizer
    ```

2.  **Directly select a project by name:**
    If you know the name (or part of the name) of the project you want to open:
    ```bash
    tmux-sessionizer my_project_name
    ```

3.  **Directly select a project by path:**
    You can also provide a path (absolute or relative) to a project:
    ```bash
    tmux-sessionizer ~/Development/specific-project
    # or
    tmux-sessionizer ./local-project
    ```

4.  **Run with debug logging:**
    To see detailed logs of what the application is doing:
    ```bash
    tmux-sessionizer --debug
    # or with a direct selection
    tmux-sessionizer --debug my_project_name
    ```

## Configuration

### Search Paths

Currently, `tmux-sessionizer` scans a predefined set of directories for projects. These default search paths are:

*   `~/Development`
*   `~/Development/raganw`
*   `~/.config`

Tilde (`~`) is automatically expanded to your home directory.

*Future versions may allow customization of these search paths and exclusion patterns via a configuration file.*

## Development Status

This project is currently **in progress**. Core functionalities are being implemented.
