# check-droid-review-config

Status: pass

## Why This Matters

Droid workflows handle repository secrets and automated review or security output; invariant drift can expose secrets, break BYOK model selection, or degrade review quality.

## Violations

None detected.

## Rerun

```bash
cargo xtask check-droid-review-config
```
