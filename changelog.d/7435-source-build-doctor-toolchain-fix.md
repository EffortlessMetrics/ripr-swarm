<!-- section: Changed -->
- Doctor: `ripr doctor --profile source-build` names an install route on the
  `!` line when cargo or rustc is missing (`install Rust via rustup
  (https://rustup.rs) or your OS package manager`), and no longer prints the
  analysis-first-run start-here block or `Recommended first command: ripr
  check`. Wait/pipe-reader failure for cargo or rustc names `could not be
  waited on` and does not prescribe rustup.rs. The analysis profile is
  unchanged. A rustc below the build minimum still names `rustup update
  stable`
  ([#7435](https://github.com/EffortlessMetrics/ripr-swarm/issues/7435)).
