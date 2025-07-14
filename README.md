# xgit

An enhanced Git tool built with Rust that provides AI-powered commit messages and interactive branch switching.

## Features

- **AI-powered commit messages** - Generate conventional commit messages using AI
- **Interactive branch selection** - Easy branch switching with a visual picker
- **Passthrough support** - Works seamlessly with existing git commit workflows

## Installation

```bash
cargo install xgit
```

## Usage

### Branch Switching
Interactive branch selection and switching:
```bash
xgit branch
```

### AI-Powered Commits
Generate commit messages automatically using AI:
```bash
# Stage your changes first
git add .

# Use AI to generate commit message
xgit commit
```

## Development 

1. Clone the repository:
   ```bash
   git clone <repo-url>
   cd xgit
   ```

2. Install git hooks (recommended):
   ```bash
   ./scripts/install-hooks.sh
   ```

