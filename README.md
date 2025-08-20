# tmux-sessionizer

## Purpose

`tmux-sessionizer` is a Rust utility designed to simplify managing tmux sessions. It helps you quickly find and switch to project directories, automatically creating or attaching to tmux sessions named after those directories. This project is a Rust rewrite of an original bash script, aiming for improved performance and maintainability.

## Description

The tool scans specified search paths for project directories. It can identify Git repositories and worktrees, presenting them in a fuzzy finder interface (powered by Skim) for easy selection. Once a project is selected, `tmux-sessionizer` will either create a new tmux session for it or switch/attach to an existing one. Session names are intelligently derived from the project directory or Git worktree structure.

## Features

- **Fuzzy Project Selection**: Quickly find projects using an interactive fuzzy finder.
- **Git Integration**: Automatically detects Git repositories and worktrees, providing enhanced display names (e.g., `repo_name (worktree_name)`).
- **Smart Session Naming**: Generates clean and descriptive tmux session names.
- **Automatic Session Management**: Creates new tmux sessions or attaches to existing ones.
- **Direct Selection**: Optionally bypass the fuzzy finder by providing a project name or path directly.
- **Configurable Search Paths**: Scans predefined common development directories (currently `~/.config`). (Future: customizable via config file).

## Requirements

To build and use `tmux-sessionizer`, you need the following installed on your system:

- **Rust toolchain**: (e.g., via [rustup](https://rustup.rs/)) for building the application.
- **tmux**: The terminal multiplexer itself. The application interacts with `tmux` to manage sessions.
- **git**: Required for Git repository and worktree detection features.

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

- `[DIRECT_SELECTION]` (Positional Argument)

  - Specifies a direct path or name to select.
  - If provided, the fuzzy finder interface is skipped.
  - The application will attempt to find a unique match among the scanned project directories based on this argument. If a unique match is found, it will be selected. If multiple matches are found or no match is found, an appropriate message will be displayed.
  - Example: `tmux-sessionizer my_project` or `tmux-sessionizer ~/Development/another_project`

- `-d, --debug`
  - Enables detailed debug logging output.
  - Useful for troubleshooting or understanding the application's behavior.

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

`tmux-sessionizer` can be configured using a TOML file located at `~/.config/tmux-sessionizer/tmux-sessionizer.toml`. This file allows you to customize search paths and exclusion patterns.

### Configuration File Location

The configuration file is expected at:

```
~/.config/tmux-sessionizer/tmux-sessionizer.toml
```

If this file does not exist, `tmux-sessionizer` will use default settings. Tilde (`~`) is expanded to your home directory.

### Configuration Options

The following options can be set in the TOML file:

- **`search_paths`** (Optional, Array of Strings)

  - Defines the primary directories to scan for projects.
  - Paths starting with `~` will be expanded to your home directory.
  - If not specified in the config file, default paths will be used (currently `~/.config`).
  - Example:
    ```toml
    # ~/.config/tmux-sessionizer/tmux-sessionizer.toml
    search_paths = ["~/dev", "~/workspaces", "/opt/projects"]
    ```

- **`additional_paths`** (Optional, Array of Strings)

  - Specifies extra directories to include in the scan, in addition to `search_paths`.
  - Useful for temporarily adding project locations.
  - Paths starting with `~` will be expanded.
  - Example:
    ```toml
    # ~/.config/tmux-sessionizer/tmux-sessionizer.toml
    additional_paths = ["~/clients/project-x", "/mnt/shared/team-projects"]
    ```

- **`exclude_patterns`** (Optional, Array of Strings)
  - A list of regular expressions. Any directory whose _full, absolute path_ matches one of these patterns will be excluded.
  - Useful for ignoring common build/dependency directories (e.g., `node_modules`, `target`) or specific projects.
  - Remember to escape special regex characters if needed (e.g., `.` should be `\.`).
  - Example:
    ```toml
    # ~/.config/tmux-sessionizer/tmux-sessionizer.toml
    exclude_patterns = [
      "/node_modules/",
      "/target/",
      "/vendor/",
      "/\.git/",        # Exclude .git directories themselves
      "/\.cache/",
      "/__pycache__/",
      "/\.venv/",
      "/path/to/ignore/this/project", # Ignore a specific project path
    ]
    ```

### Example Configuration File

See the `examples/tmux-sessionizer.toml` file in the repository for a detailed example with comments explaining each option.

### Configuration Precedence

Settings are applied in the following order, with later sources overriding earlier ones:

1.  **Defaults**: Built-in default values (e.g., default search paths if `search_paths` is not set in the config file).
2.  **Configuration File**: Values loaded from `~/.config/tmux-sessionizer/tmux-sessionizer.toml`.
3.  **Command-Line Arguments**: Arguments provided when running the application (e.g., `--debug`, `[DIRECT_SELECTION]`). _Note: Currently, CLI arguments do not override paths or exclusions from the config file, but this defines the intended future precedence._

### Troubleshooting

- **File Not Found**: Ensure the configuration file is placed exactly at `~/.config/tmux-sessionizer/tmux-sessionizer.toml`. Check permissions if the file exists but cannot be read.
- **Invalid TOML**: Check the syntax of your TOML file. Errors during parsing will be logged if `--debug` is enabled.
- **Path Issues**: Ensure specified paths exist and are directories. Errors related to path validation will be logged.
- **Permissions**: Make sure `tmux-sessionizer` has read permissions for the configuration file and execute/search permissions for the directories it needs to scan.

## Releases

This project uses automated release management through GitHub Actions:

### Manual Release Process

A "Cut Release" workflow allows maintainers to manually trigger version bumps and releases:

1. Go to the **Actions** tab in the GitHub repository
2. Select **Cut Release** workflow
3. Click **Run workflow** 
4. Choose the version bump type:
   - **patch**: For bug fixes (e.g., 0.3.1 → 0.3.2)
   - **minor**: For new features (e.g., 0.3.1 → 0.4.0)  
   - **major**: For breaking changes (e.g., 0.3.1 → 1.0.0)
   - **custom**: Specify an exact version number

The workflow will:
- Update the version in `Cargo.toml` and `Cargo.lock`
- Commit the changes directly to the main branch (bypassing branch protection)
- Create and push a git tag (e.g., `v0.3.2`)
- The tag creation triggers the automated release build workflow

**Note**: This workflow bypasses the main branch protection by using a Personal Access Token (PAT) with appropriate permissions.

#### Required Repository Configuration

For the Cut Release workflow to function properly, configure the following repository settings:

1. **Personal Access Token (PAT)**: Create a fine-grained PAT to bypass branch protection
   - Go to **GitHub Settings → Developer settings → Personal access tokens → Fine-grained tokens**
   - Click **Generate new token**
   - Select this repository in **Repository access**
   - Under **Permissions**, grant:
     - **Contents**: Write access (to push commits and tags)
     - **Metadata**: Read access (basic repository access)
   - Copy the token and add it as a repository secret named `RELEASE_PAT`
   - Go to repository **Settings → Secrets and variables → Actions**
   - Click **New repository secret**, name it `RELEASE_PAT`, and paste the token

2. **Branch Protection Bypass**: Ensure the PAT owner has admin access to bypass branch protection rules

If the Cut Release workflow fails with permission errors, verify the above PAT configuration.

### Automated Release Build

When a version tag is pushed (via the automated tag workflow after PR merge or manually), the release workflow automatically:
- Builds binaries for Linux (x86_64), macOS (x86_64 and ARM64)
- Creates GitHub releases with downloadable assets
- Updates the Homebrew tap formula

## Development Status

This project is currently **in progress**. Core functionalities are being implemented.
