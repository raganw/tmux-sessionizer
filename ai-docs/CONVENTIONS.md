# Rust Project Best Practices and Conventions

## Table of Contents

- [Code Organization](#code-organization)
- [Naming Conventions](#naming-conventions)
- [Documentation](#documentation)
- [Error Handling](#error-handling)
- [Testing](#testing)
- [Performance Considerations](#performance-considerations)
- [Dependency Management](#dependency-management)
- [Tooling](#tooling)
- [Git Practices](#git-practices)
- [CI/CD Recommendations](#cicd-recommendations)

## Code Organization

### Project Structure

```
project_name/
├── .github/            # GitHub workflows and templates
├── .vscode/            # VS Code configuration
├── benches/            # Benchmarks
├── examples/           # Example code showcasing library usage
├── src/
│   ├── bin/            # Binaries/executables
│   ├── lib.rs          # Library root
│   └── main.rs         # Main application entry point
├── tests/              # Integration tests
├── .gitignore
├── .rustfmt.toml       # Formatter configuration
├── Cargo.lock
├── Cargo.toml          # Package manifest
└── README.md
```

### Module Organization

- Use `mod` declarations in your root files (`lib.rs`, `main.rs`)
- Prefer one module per file for clarity
- Keep module hierarchy shallow (max 3-4 levels deep)
- Use `pub(crate)` for items that should be visible within the crate but not exported

## Naming Conventions

### General Rules

- **Crates**: `snake_case`
- **Modules**: `snake_case`
- **Types** (structs, enums, unions, traits): `PascalCase`
- **Functions**, **methods**, **variables**: `snake_case`
- **Constants**: `SCREAMING_SNAKE_CASE`
- **Statics**: `SCREAMING_SNAKE_CASE`
- **Type parameters**: `PascalCase`, usually single uppercase letters like `T`, `E`
- **Lifetimes**: short lowercase like `'a`, `'db`

### Semantic Naming

- Boolean variables should have prefixes like `is_`, `has_`, `should_`
- Methods that convert between types should use `to_` prefix (`to_string()`, `to_vec()`)
- Builder pattern methods should match the field name (`name()`, `size()`)

## Documentation

### Code Documentation

- Document all public items with doc comments (`///`)
- Use Markdown in doc comments
- Include examples in doc comments for public APIs
- Document panics, errors, and safety considerations
- Avoid superfluous comments; focus on explaining the "why" rather than the "what"

Example:

````rust
/// Divides two numbers.
///
/// # Examples
///
/// ```
/// let result = my_crate::divide(10.0, 2.0);
/// assert_eq!(result, Ok(5.0));
/// ```
///
/// # Errors
///
/// Returns `Err` if `divisor` is zero.
pub fn divide(dividend: f64, divisor: f64) -> Result<f64, DivisionError> {
    if divisor == 0.0 {
        Err(DivisionError::DivideByZero)
    } else {
        Ok(dividend / divisor)
    }
}
````

### Project Documentation

- `README.md` should include:
  - Project description
  - Installation instructions
  - Basic usage examples
  - Link to more detailed documentation
  - License information

## Error Handling

### Error Types

- Use `Result<T, E>` for operations that can fail
- Create custom error types for your library/application
- Implement `std::error::Error` for your error types
- Make errors descriptive and actionable

### Error Pattern

```rust
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    Parse(#[from] std::num::ParseIntError),

    #[error("Configuration error: {message}")]
    Config { message: String },
}

// Use anyhow for application code
use anyhow::{Context, Result};

fn read_config(path: &str) -> Result<Config> {
    std::fs::read_to_string(path)
        .context("Failed to read config file")?
        .parse()
        .context("Failed to parse config file")
}
```

### Best Practices

- Prefer `?` operator over `match` for error propagation
- Use `thiserror` for defining error types in libraries
- Use `anyhow` for application error handling
- Avoid `unwrap()` and `expect()` in production code
- Consider `#[non_exhaustive]` for error enums that might grow

## Testing

### Unit Tests

- Place unit tests in the same file as the code they test using `#[cfg(test)]` module
- Name test functions descriptively: `test_<function_name>_<scenario>`
- Use appropriate assertions: `assert!`, `assert_eq!`, `assert_ne!`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_positive_numbers() {
        assert_eq!(add(2, 3), 5);
    }

    #[test]
    fn test_add_negative_numbers() {
        assert_eq!(add(-2, -3), -5);
    }
}
```

### Integration Tests

- Place integration tests in the `tests/` directory
- Each file in `tests/` is compiled as a separate binary
- Focus on testing the public API of your crate

### Property-Based Testing

- Consider using `proptest` or `quickcheck` for property-based testing
- Define properties that should hold for all valid inputs

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_reverse_reverse_is_identity(s in ".*") {
        let twice_reversed = reverse(&reverse(&s));
        prop_assert_eq!(s, twice_reversed);
    }
}
```

## Performance Considerations

### Benchmarking

- Use `criterion` for benchmarking
- Benchmark critical paths and potential bottlenecks
- Compare implementations with different algorithms or data structures

### Memory Usage

- Prefer stack allocation over heap when possible
- Use `Box<T>` for heap allocation of single values
- Consider custom allocators for specific needs
- Use Rust's ownership model to minimize copying

### Optimization Tips

- Prefer iterators over explicit loops for clarity and potential optimization
- Use `#[inline]` for small, frequently called functions
- Consider SIMD for performance-critical numeric operations
- Profile before optimizing using tools like `perf` or `flamegraph`

## Dependency Management

### Selection Criteria

- Evaluate crates for:
  - Maintenance status and update frequency
  - Documentation quality
  - Test coverage
  - Dependency tree size
  - License compatibility

### Version Specification

- Pin dependencies to compatible versions: `^0.5.0` or `0.5`
- For critical dependencies, consider exact versions: `=0.5.0`
- Regularly update dependencies for security fixes

### Features

- Use feature flags to make functionality optional
- Keep default features minimal
- Document features in your `Cargo.toml`

```toml
[dependencies]
serde = { version = "1.0", features = ["derive"], optional = true }

[features]
default = ["std"]
std = ["serde"]
serialization = ["serde"]
```

## Tooling

### Essential Tools

- **rustfmt**: Format code consistently
- **clippy**: Catch common mistakes and improve code
- **rust-analyzer**: Provide IDE support

### Configuration

- `.rustfmt.toml` for formatting rules:

```toml
edition = "2021"
max_width = 100
tab_spaces = 4
```

- `clippy.toml` for linter rules:

```toml
too-many-arguments-threshold = 8
```

### Recommended Commands

```bash
# Format all code
cargo fmt

# Run clippy with pedantic lints
cargo clippy -- -W clippy::pedantic

# Check documentation
cargo doc --no-deps --open

# Run tests with coverage (using cargo-tarpaulin)
cargo tarpaulin

# Audit dependencies for vulnerabilities
cargo audit
```

## Git Practices

### Commit Messages

- Use conventional commits format:
  - `feat:` New features
  - `fix:` Bug fixes
  - `docs:` Documentation changes
  - `refactor:` Code refactoring
  - `test:` Adding or modifying tests
  - `chore:` Maintenance tasks

### Branching Strategy

- `main`: Stable, always deployable
- `dev`: Development branch
- Feature branches: `feature/short-description`
- Bug fixes: `fix/issue-description`

### Pre-commit Hooks

Set up hooks to run before each commit:

- `cargo fmt`
- `cargo clippy`
- `cargo test`

## CI/CD Recommendations

### GitHub Actions Workflow

```yaml
name: CI

on:
  push:
    branches: [main, dev]
  pull_request:
    branches: [main, dev]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          profile: minimal
          toolchain: stable
      - name: Build
        run: cargo build --verbose
      - name: Run tests
        run: cargo test --verbose
      - name: Run clippy
        run: cargo clippy -- -D warnings
      - name: Check formatting
        run: cargo fmt -- --check
      - name: Security audit
        run: cargo audit
```

### Release Process

1. Update version in `Cargo.toml`
2. Update `CHANGELOG.md`
3. Create a git tag with the version
4. Publish to crates.io: `cargo publish`

## Additional Resources

### Recommended Reading

- [The Rust Book](https://doc.rust-lang.org/book/)
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [Rust Design Patterns](https://rust-unofficial.github.io/patterns/)

### Useful Crates

- **serde**: Serialization/deserialization
- **tokio**: Async runtime
- **rayon**: Parallel computing
- **clap**: Command line parsing
- **log + env_logger**: Logging
- **thiserror + anyhow**: Error handling

