# check-evidence-promotion-honesty

Status: pass

## Why This Matters

A finding may not be promoted to `exposed` unless its evidence STRUCTURALLY matches the seam. Each confirmed fake-clean is pinned as a charter member that must stay non-promoted. Honest re-bless of a charter fixture to `exposed` in the golden would bypass `goldens check`; this gate reads the byte-pinned golden and asserts the semantic expectation independently.

## Violations

None detected.


## Detail

pass: corpus tiers valid; all charter members at expected class; no clean-guard case lost its findings; scope-guard cases kept report scope headers; no promoted case carries exposed; all controls retain exposed

## Rerun

```bash
cargo xtask check-evidence-promotion-honesty
```
