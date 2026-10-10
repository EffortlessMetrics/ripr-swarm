# check-rust-source-role-authority

Status: pass

## Why This Matters

Source-role fixes have repeatedly landed in one producer or consumer while another path retained an older heuristic; a mechanical authority gate keeps every consumer on the producer-owned role contract.

## Violations

None detected.

## Rerun

```bash
cargo xtask check-rust-source-role-authority
```
