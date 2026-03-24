# xgit

An enhanced Git tool built with Rust that provides AI-powered commit messages, interactive branch management, and GitHub PR integration.

## Features

- **🤖 AI-powered commit messages** - Generate conventional commit messages using AI
- **🌿 Interactive branch management** - Easy branch switching with a visual picker
- **📊 Branch statistics with GitHub PR tracking** - View branch status, merge state, and associated GitHub PRs
- **🗑️ Smart branch pruning** - Clean up merged branches with safety checks and interactive selection
- **🔗 GitHub integration** - Automatically detect and display pull request information
- **🚀 Git passthrough** - Works seamlessly with existing git workflows

## Installation

### macOS

Install with Homebrew:

```bash
brew tap LiXuanqi/xgit
brew install xgit
```

### Linux

Install the latest prebuilt `musl` binary:

```bash
curl -fsSL https://raw.githubusercontent.com/LiXuanqi/xgit/main/scripts/install.sh | sh
```

> [!NOTE]
> `musl` is a Linux C standard library implementation that makes it easier to ship portable prebuilt binaries. In practice, this means the downloaded `xg` binary should run on most Linux distributions without requiring you to install extra runtime libraries.

By default, the installer puts `xg` in `~/.local/bin`.

Install a specific version:

```bash
curl -fsSL https://raw.githubusercontent.com/LiXuanqi/xgit/main/scripts/install.sh | sh -s -- --version v0.2.6
```

Install to a custom directory:

```bash
curl -fsSL https://raw.githubusercontent.com/LiXuanqi/xgit/main/scripts/install.sh | sh -s -- --dir /usr/local/bin
```

### Cargo

Rust users can also install from crates.io:

```bash
cargo install xgit --bin xg
```

## Usage

```bash
xg <command>
```

### Interactive Branch Switching

```bash
xg branch
xg b
```

### Branch Statistics

```bash
xg branch --stats
xg b --stats
```

### Smart Branch Pruning

```bash
xg branch --prune-merged --dry-run
xg b --prune-merged --dry-run

xg branch --prune-merged
xg b --prune-merged
```

### AI-Powered Commits

```bash
git add .
xg commit
xg c
```

### Git Passthrough

```bash
xg git status
xg git log
xg git diff
```

## GitHub Integration

`xg` uses the GitHub CLI for PR operations in the current default backend. Install and authenticate `gh` if you want PR features such as `xg diff`.

## Development

```bash
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
```

## Release Flow

Push a version tag and GitHub Actions will build Linux `musl` binaries and publish a GitHub Release automatically.

```bash
git tag v0.2.6
git push origin v0.2.6
```

Release assets produced by the workflow:

- `xg-v0.2.6-x86_64-unknown-linux-musl.tar.gz`
- `xg-v0.2.6-aarch64-unknown-linux-musl.tar.gz`
