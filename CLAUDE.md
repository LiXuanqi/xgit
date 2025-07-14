# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Rust project named "xgit" using Rust edition 2024. It's a Git extension tool that provides enhanced branch management and other Git workflow improvements.

## Common Commands

- **Build**: `cargo build`
- **Run**: `cargo run`
- **Test**: `cargo test`
- **Check**: `cargo check`
- **Format**: `cargo fmt`
- **Lint**: `cargo clippy --all-targets -- -D warnings`

## Code Style Rules

When writing or editing Rust code in this project, follow these strict rules:

1. **Clippy Compliance**: All code must pass `cargo clippy --all-targets -- -D warnings` without any warnings
2. **Format Strings**: Always use inlined format arguments (e.g., `format!("Hello {name}")` not `format!("Hello {}", name)`)
3. **No Unnecessary Borrows**: Avoid `&` when passing arrays to `.args()` method
4. **Use `strip_prefix()`**: Replace manual string slicing with `.strip_prefix()` method when appropriate
5. **Combine Conditions**: Use `&&` instead of nested if statements where possible
6. **No Comments**: DO NOT add code comments unless explicitly requested by the user

## Testing

- Run `cargo test` before committing changes
- Tests should use `assert_fs` for temporary directory setup
- Git operations in tests should create proper repository state (commits, remotes, etc.)

## Architecture

- `src/main.rs`: Entry point with external command handling and allowlist
- `src/cli.rs`: Command-line interface definitions using clap
- `src/git_repo.rs`: Git operations wrapper using git2 crate
- `src/commands/`: Individual command implementations
- `scripts/pre-commit`: Pre-commit hook with auto-formatting

## Git Integration

Prefer git2 crate over shell commands for Git operations. Use existing GitRepo helper methods when available.