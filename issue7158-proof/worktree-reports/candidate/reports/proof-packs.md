# check-proof-packs

Status: pass

## Why This Matters

policy/proof-packs.toml is the routing unit for proof-aware validation (docs/PROOF_ROUTING.md). While state is manifest-only, nothing routes on it yet, but the manifest must stay parseable, name only real repo commands and CI lanes, and keep the release-package pack pinned to full proof before any routing behavior consumes it.

## Violations

None detected.

## Rerun

```bash
cargo xtask check-proof-packs
```
