<!-- section: Fixed -->
- A `[[test]]` target that `cargo test` never runs (`test = false`, `harness = false`, or `required-features` outside the default features) no longer earns test credit, so a changed owner it appears to pin no longer reads `exposed` (#8016).
