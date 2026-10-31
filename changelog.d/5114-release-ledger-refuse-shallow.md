<!-- section: Internal / CI -->
- `cargo xtask release-ledger capture` and `finalize` now refuse a shallow clone with a "full clone required" error that names `git fetch --unshallow`, instead of writing truncated facts that later fail `derive` (#5114).
