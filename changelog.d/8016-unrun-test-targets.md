<!-- section: Fixed -->
- A `[[test]]` target with `test = false`, or with `required-features` outside
  the default feature set, is no longer credited as test evidence: `cargo test`
  never runs it. `harness = false` stays with the harness registry (#8016).
