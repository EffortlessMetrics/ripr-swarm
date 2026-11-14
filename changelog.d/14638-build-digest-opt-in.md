<!-- section: Internal / CI -->
- `build.rs` bakes `RIPR_BUILD_SOURCE_DIGEST` only for a build that sets
  `RIPR_PERSISTED_CACHE=1`, so a dev or test build's compile-time environment no
  longer changes on every source edit and sccache can reuse compilation units;
  persisted analysis caches still key on a per-tree digest when the flag is set,
  and every sccache-enabled job in `rust-gates.yml` now sets
  `CARGO_INCREMENTAL: 0` (#14638).
