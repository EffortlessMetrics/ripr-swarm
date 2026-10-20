# check-text-encoding

Status: pass

## Why This Matters

A UTF-8 BOM is invisible in review but shifts the first bytes of a file, so a later gate fails with a misleading error: serde rejects the JSON, a first-line heading or allowlist entry is missed, and rustc sees a stray token. Hand-resolved merge files must be saved as UTF-8 without a BOM.

## Violations

None detected.

## Rerun

```bash
cargo xtask check-text-encoding
```
