# AGENTS.md

## Stacked PR Sync Policy

- Default behavior for `xgit diff` is **force-with-lease sync** for existing PRs.
- For a commit already mapped to a PR, `xgit` force-pushes the local mapped commit
  to the PR head branch.

## Why This Is Default

- Keeps stacked PR ancestry clean and predictable.
- Avoids commit-list pollution across stacked PRs.
- Ensures PR branch history matches local mapped commit semantics.

## Known Tradeoffs

- Force-push rewrites PR branch history.
- Review rounds are not represented as appended commits by default.

## If Problems Appear

- Consider adding an optional append/synthetic mode for specific workflows.
