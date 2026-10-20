# check-changelog-fragments

Status: pass

## Why This Matters

Fragments are folded into CHANGELOG.md by hand at the release cut. A fragment with no section, an unknown section, or no issue/PR reference cannot be placed or traced, a fragment left unfolded drops a user-facing change from the release notes, an Unreleased bullet with no issue, PR, or RIPR-SPEC reference is untraceable after the cut, and verdict-corpus or Repo ops bookkeeping in a user-facing section crowds the notes an upgrader reads.

## Violations

None detected.

## Rerun

```bash
cargo xtask check-changelog-fragments
```
