<!-- section: Fixed -->
- `ripr agent verify-execute` preserves an existing result when another writer
  publishes concurrently or the destination is a dangling link (#2332).
- `ripr review-comments` records a terminal failure when `--gap-ledger` and
  `--check-output` are supplied together (#1604).
