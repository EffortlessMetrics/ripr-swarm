<!-- section: Fixed -->
- `ripr_prepare_repair` re-verifies consumed-source custody like `ripr_get_repair_card` and refuses with `stale_snapshot` when a consumed source or test file changed or was deleted after the snapshot, instead of minting a ready repair transaction (#15240).
