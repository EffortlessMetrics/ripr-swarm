# check-covered-by

Status: pass

## Why This Matters

A `covered_by` entry that names a renamed or deleted test is a false-confidence receipt: nothing proves the suppressed surface is still exercised. An `expires` date or required field that the ledger records but no gate reads is the same class of unread contract.

## Violations

None detected.

## Rerun

```bash
cargo xtask check-covered-by
```
