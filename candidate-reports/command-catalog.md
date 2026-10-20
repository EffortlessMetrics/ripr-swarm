# check-command-catalog

Status: pass

## Why This Matters

The command mutability catalog is the repo-ops map for agents. Every xtask command must stay classified so workers know what is safe to run, what writes generated evidence, what requires judgment, and which checks a CI workflow enforces.

## Violations

None detected.

## Rerun

```bash
cargo xtask check-command-catalog
```
