<!-- section: Fixed -->
- `ripr gate evaluate` preserves parser-declined files when both `--pr-guidance`
  and `--gap-ledger` are supplied. In calibrated-gate mode, a declined changed
  file now blocks with exit 3 even when the ledger has no candidates (#14494).
