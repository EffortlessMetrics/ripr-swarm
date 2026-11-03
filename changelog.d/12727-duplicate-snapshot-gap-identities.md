<!-- section: Internal / CI -->
- `cargo xtask ripr-pr` withholds a base or head snapshot that repeats a `canonical_gap_id` as unavailable, with a reason naming the side, instead of letting the last duplicate row win the lookup and report the earlier row as resolved under complete coverage; the downstream rejection of duplicate output ids is now pinned by a regression test (#12727).
