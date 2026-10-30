<!-- section: Internal / CI -->
- `cargo xtask check-fast` reports its local base, head and merge-base identities
  and rejects changes to that selector during a run, including changes that
  retain the same filenames. Gate receipts must name the same selector; offline
  and local authoring workflows remain supported (#3549).
