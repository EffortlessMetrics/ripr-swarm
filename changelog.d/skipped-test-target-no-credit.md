<!-- section: Fixed -->
- A test in a declared `[[test]]` target that `cargo test` never runs
  (`test = false`, `harness = false` with no registered custom harness, or
  `required-features` unmet under cargo defaults) is no longer credited as
  evidence, so it can no longer read `exposed` (#8016).
