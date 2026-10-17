<!-- section: Added -->
- `cargo xtask real-crate-dogfood`: 21 pinned recent commits of 14 popular Rust crates
  (`benchmarks/real_crate_dogfood`), analyzed with `ripr check`, so related-test naming,
  duplicate test rows, findings on deleted lines and wall time on real diffs can be measured
  before and after an analyzer change. Fetching is opt-in network; counts are not verdicts.
