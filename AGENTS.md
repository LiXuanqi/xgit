# AGENTS.md

## Stacked PR Sync Policy

- Default behavior for `xgit diff` is **synthetic sync** for existing PRs.
- For a commit already mapped to a PR, `xgit` creates a synthetic commit:
  - parent = current remote PR head commit
  - tree = current local mapped commit tree
- Then `xgit` performs a normal push (no force-push) to the PR head branch.

## Why This Is Default

- Keeps review rounds visible as separate remote commits.
- Avoids history rewrite on the PR branch during iteration.
- Matches current team preference while this model is being evaluated.

## Known Tradeoffs

- Remote PR branch SHA diverges from local rewritten SHA by design.
- PR branches can accumulate synthetic commit chains over time.
- Debugging should use trailer/PR mapping rather than strict SHA equality.

## If Problems Appear

- Re-evaluate and optionally switch back to force-with-lease sync.
- Consider adding a compaction/squash command for long synthetic chains.
