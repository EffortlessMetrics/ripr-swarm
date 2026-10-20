# check-generated-clean

Status: pass

## Why This Matters

Generated evidence and build residue should not leak into ordinary PR diffs. Public badge endpoint counts are generated trust markers, and target artifacts are local/CI outputs.

## Violations

None detected.

## Rerun

```bash
cargo xtask check-generated-clean
```
