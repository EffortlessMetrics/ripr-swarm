<!-- section: Fixed -->
- `cargo xtask release-ledger capture` and `finalize` now stop on a shallow clone and say so, with the `git fetch --unshallow origin` fix, instead of failing with an opaque `Invalid revision range` from `git rev-list` (#2768).
