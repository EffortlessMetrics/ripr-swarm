# check-release-targets (distribution contract)

Status: pass

## Why This Matters

PyPI, npm, native archives, and installed-product qualification must refer to one product identity, version source, feature set, and target map. Drift here can publish the right name with the wrong binary or silently pair one launcher version with another payload.

## Violations

None detected.

## Rerun

```bash
cargo xtask check-release-targets
```
