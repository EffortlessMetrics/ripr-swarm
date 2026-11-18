<!-- section: Fixed -->
- A `[[bin]]` or `[[test]]` target whose `required-features` another
  workspace member enables through a path dependency (`b` depending on `a =
  { path = "../a", features = ["special"] }`) is no longer read as skipped.
  A workspace-wide `cargo test` unifies features across members and runs
  that target's tests, yet ripr withheld their credit and read the changed
  owner as `no_static_path`; it now keeps the credit. A request for another
  feature, an optional dependency left off, a build dependency and a
  feature chain the dependent does not enable keep the skip
  ([#15079](https://github.com/EffortlessMetrics/ripr-swarm/issues/15079)).
