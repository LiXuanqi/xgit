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

```bash
cargo install xgit
```

## Updating

To update to the latest version:

```bash
cargo install xgit --force
```

**Note:** The `--force` flag is required to overwrite the existing installation.

## Usage

### Interactive Branch Switching
Select and switch between branches interactively:
```bash
xgit branch
# or use the short alias:
xgit b
```

### Branch Statistics & GitHub PR Tracking
View comprehensive branch information including GitHub PR status:
```bash
xgit branch --stats
# or use the short alias:
xgit b --stats
```

### Smart Branch Pruning
Clean up branches that have been merged to main:

```bash
# Preview what would be deleted (recommended first)
xgit branch --prune-merged --dry-run
# or use the short alias:
xgit b --prune-merged --dry-run

# Interactive deletion - select which branches to remove
xgit branch --prune-merged
# or use the short alias:
xgit b --prune-merged
```

### AI-Powered Commits
Generate commit messages automatically using AI:
```bash
# Stage your changes first
git add .

# Use AI to generate commit message
xgit commit
# or use the short alias:
xgit c
```

### Git Passthrough
Use any git command through xgit:
```bash
xgit status
xgit log
xgit push
# ... any git command
```

## GitHub Integration

xgit automatically detects GitHub repositories and fetches PR information for each branch. Authentication options:

1. **Environment variable**: Set `GITHUB_TOKEN`
3. **Unauthenticated**: Works with public repos (rate limited)

## Development 

1. Clone the repository:
   ```bash
   git clone https://github.com/LiXuanqi/gitx
   cd xgit
   ```

2. Install git hooks (recommended):
   ```bash
   ./scripts/install-hooks.sh
   ```

3. Build and test:
   ```bash
   cargo build
   cargo test
   cargo clippy --all-targets -- -D warnings
   ```

