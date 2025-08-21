# Semantic Release Integration Guide

This document provides information for maintainers on how the new semantic-release automation works.

## Overview

The tmux-sessionizer project now uses [semantic-release](https://github.com/semantic-release/semantic-release) for automated version management and changelog generation based on [Conventional Commits](https://www.conventionalcommits.org/).

## How It Works

1. **Commit Analysis**: When changes are pushed to `main`, semantic-release analyzes commit messages
2. **Version Determination**: Based on commit types, it automatically determines the next version:
   - `fix:` → patch version (0.5.0 → 0.5.1)
   - `feat:` → minor version (0.5.0 → 0.6.0)
   - `BREAKING CHANGE:` → major version (0.5.0 → 1.0.0)
3. **Changelog Generation**: Creates/updates `CHANGELOG.md` with release notes from commits
4. **Version Update**: Updates `Cargo.toml` and `Cargo.lock` with new version
5. **Git Operations**: Creates release commit and git tag
6. **GitHub Release**: Triggers the existing release workflow to build binaries

## Commit Message Requirements

All commits must follow the conventional commit format:

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

### Release Types:
- `feat:` - New features (minor version bump)
- `fix:` - Bug fixes (patch version bump)
- `perf:` - Performance improvements (patch version bump)

### Other Types (no release):
- `docs:` - Documentation changes
- `style:` - Code style/formatting
- `refactor:` - Code refactoring
- `test:` - Test additions/updates
- `chore:` - Maintenance tasks
- `ci:` - CI/CD changes
- `build:` - Build system changes

### Breaking Changes:
Add `BREAKING CHANGE:` in the commit footer or use `!` after the type:
```
feat!: remove deprecated fuzzy finder option
```

## Workflow Files

- **`.github/workflows/cut-release.yml`**: Main semantic-release workflow (triggers on push to main)
- **`.github/workflows/lint-pr-title.yml`**: Validates PR titles follow conventional commits
- **`.github/workflows/ci.yml`**: Includes commit message linting for push events

## Configuration Approach

The semantic-release tooling is **entirely defined within the GitHub workflow files**. No Node.js project files are added to the repository:

- **Dependencies**: npm packages are installed globally in workflow steps
- **Configuration**: Created dynamically as temporary files during workflow execution
- **Cleanup**: All temporary configuration files are automatically removed after use

This approach keeps the Rust project clean while still providing full semantic-release functionality.

## Manual Release Trigger

You can still manually trigger a release by:
1. Going to Actions → Semantic Release workflow
2. Click "Run workflow"
3. Select the main branch

Note: This still uses automated version determination based on commits since the last release.

## Migration from Manual Process

- The old manual `cut-release.yml` workflow has been replaced
- The existing `release.yml` workflow (triggered by git tags) remains unchanged
- All previous functionality is preserved while adding automation

## Troubleshooting

If semantic-release fails:
1. Check that commits follow conventional format
2. Verify RELEASE_PAT token has proper permissions
3. Check workflow logs for specific errors
4. Ensure no breaking changes in semantic-release dependencies