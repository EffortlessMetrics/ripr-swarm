<!-- section: Fixed -->
- Tests in a declared `[[test]]` target that `cargo test` never runs as
  libtest (`test = false`, `required-features` unmet under cargo defaults, or
  `harness = false` with no configured harness marker) are no longer credited
  as evidence, so a changed line they alone "cover" reads `no_static_path`
  instead of `exposed`. Other non-running shapes (`[lib] test = false`,
  examples, benches, `[[bin]] test = false`, non-default workspace members)
  stay tracked on #8016.
