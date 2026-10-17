<!-- section: Fixed -->
- A test in a `[[test]]` target that `cargo test` never runs (`test = false`,
  `required-features` unmet under default features, or `harness = false`
  without a registered custom harness) is no longer credited as exposure
  evidence (#8016).
