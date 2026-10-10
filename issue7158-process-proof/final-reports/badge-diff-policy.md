# check-badge-diff-policy

Status: pass

## Why This Matters

Public RIPR badge endpoint counts are generated trust markers. Ordinary docs, README, and implementation PRs may edit badge links or layout, but must not hand-author badges/*.json endpoint numbers.

## Violations

None detected.

## Rerun

```bash
cargo xtask check-badge-diff-policy
```
