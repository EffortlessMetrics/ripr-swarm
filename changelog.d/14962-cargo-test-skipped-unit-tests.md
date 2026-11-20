<!-- section: Fixed -->
- A Rust unit test compiled only into a library or binary that `cargo
  test` skips no longer credits a discriminator. With `[lib] test = false`
  (and likewise `[[bin]] test = false` or a bin's unmet default-feature
  `required-features`), the `#[cfg(test)]` tests in that target's module
  tree print `running 0 tests`, yet ripr read the changed owner as
  `exposed`; it now reads `no_static_path`. The skip comes from cargo
  metadata through the existing `ManifestInventory`, so an integration test
  under `tests/`, a doc example, and a module a tested bin also compiles
  keep their credit, and unavailable metadata is not a skip
  ([#14962](https://github.com/EffortlessMetrics/ripr-swarm/issues/14962)).
