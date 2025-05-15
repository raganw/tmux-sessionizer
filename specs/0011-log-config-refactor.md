## High-Level Objective

- Refactor the configuration and logging modules to centralize XDG log directory determination within the `config` module. This will allow the `logging` module to receive the target directory path, simplifying its setup and enhancing testability by allowing explicit log paths during tests.

## Mid-Level Objective

- Modify `config.rs` to determine the application's log directory path using XDG standards and store this path within the `Config` struct.
- Update `logging.rs` so that its initialization functions accept the pre-determined log directory path as a parameter, instead of resolving it internally.
- Adjust `main.rs` to retrieve the log directory path from the `Config` instance and pass it to the logging module's initialization function.
- Revise unit tests in `logging.rs` to provide a temporary directory path directly to the logging functions, removing the need for XDG-related environment mocking in those tests.
- Add or update unit tests in `config.rs` to verify the correct determination of the log directory path.

## Implementation Notes

- `config.rs` will use the `cross-xdg` crate to find the appropriate XDG data directory.
- The `Config` struct will gain a new field, for example `log_directory: PathBuf`, to store the resolved path.
- The `APP_NAME` constant, used for the subdirectory and log file prefix, will be needed in `config.rs` for directory path construction and in `logging.rs` for the log file name prefix. It can be defined in both or moved to a shared location. For this plan, assume `config.rs` will define/use it for the directory, and `logging.rs` will retain its own for the filename prefix.
- The responsibility of creating the log directory (`fs::create_dir_all`) will remain within the `logging` module, operating on the path provided to it.
- Error handling for XDG path determination will be in `config.rs`, while error handling for log directory creation and file appender setup will remain in `logging.rs`.

## Context

### Files to be modified:

- `src/config.rs`
- `src/logging.rs`
- `src/main.rs`

## Low-Level Tasks

> Ordered from start to finish

1.  **Update `Config` struct and initialization in `config.rs`**

    - Define `APP_NAME` constant in `src/config.rs` (e.g., `const APP_NAME: &str = "tmux-sessionizer";`).
    - Add a new public field `log_directory: PathBuf` to the `Config` struct.
    - In `Config::new()` (or a helper function called by it):
      - Use `cross_xdg::BaseDirs::new()` to get XDG directories.
      - Construct the log directory path (e.g., `base_dirs.data_home().join(APP_NAME)`).
      - Store this `PathBuf` in the `config.log_directory` field.
      - Return an appropriate `ConfigError` if XDG directories cannot be determined.

2.  **Refactor `logging.rs` to accept the log directory path**

    - Modify the `init_file_subscriber` function:
      - Change its signature to accept `log_directory: &PathBuf` (or `&Path`) as a parameter.
      - Remove the internal XDG directory determination logic (current steps 1 and 2 involving `BaseDirs` and `data_home.join(APP_NAME)`).
      - Use the provided `log_directory` parameter directly for `fs::create_dir_all` and as the argument to `RollingFileAppender::builder().build()`.
      - Retain the use of its local `APP_NAME` constant for `filename_prefix`.
    - Modify the public `init_global_tracing` function:
      - Change its signature to accept `log_directory: &PathBuf` (or `&Path`) in addition to `level: &str`.
      - Pass the received `log_directory` to `init_file_subscriber`.
    - Modify the test helper function `init_tracing` (if kept, or adapt tests directly):
      - Change its signature to accept `log_directory: &PathBuf` (or `&Path`) in addition to `level: &str`.
      - Pass the received `log_directory` to `init_file_subscriber`.

3.  **Update `main.rs` to pass the log directory path**

    - In `fn main()`:
      - After `let config = Config::new()?;` successfully initializes, access `config.log_directory`.
      - Call `logging::init_global_tracing` with `&config.log_directory` and the log level string.
      - The `eprintln!` message in `logging::init_global_tracing` about the log directory path will now correctly reflect the path determined by `config`.

4.  **Update unit tests in `logging.rs`**

    - For tests like `test_xdg_directory_determination_and_creation`, `test_log_file_creation`, and `test_debug_mode_level_setting`:
      - Remove setup related to mocking `XDG_DATA_HOME` environment variable, as `logging.rs` no longer reads it.
      - Instead, create a `tempfile::tempdir()` and construct a `PathBuf` to a desired log location within this temporary directory.
      - Pass this `PathBuf` to the modified `init_tracing` (or directly to `init_file_subscriber` if tests are refactored to call it).
      - Adjust assertions to verify log file creation and content in the explicitly provided temporary directory.
      - The `get_expected_log_dir` helper function will likely take the temporary base path as an argument or be replaced by direct path construction in tests.

5.  **Add/Update unit tests in `config.rs`**

    - Create new unit tests specifically for the log directory determination logic within `Config::new()`.
    - These tests _should_ involve manipulating/mocking `XDG_DATA_HOME` (if `cross-xdg` respects it for testing, or by mocking `BaseDirs::new()` if possible/necessary) to ensure `config.log_directory` is correctly populated under various conditions (e.g., `XDG_DATA_HOME` set, not set).
    - Verify that `ConfigError` is returned appropriately if paths cannot be determined.

6.  **Final review and cleanup**
    - Ensure all modified functions and struct fields have updated documentation comments reflecting the changes in responsibility and parameters.
    - Verify that error propagation paths are correct (e.g., XDG errors from `config`, file system errors from `logging`).
    - Confirm that `APP_NAME` is used consistently for directory naming in `config.rs` and filename prefixing in `logging.rs`.
    - Ensure the code adheres to conventions outlined in `CONVENTIONS.md`.
