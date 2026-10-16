<!-- section: Fixed -->
- A test in a `[[test]]` target that `cargo test` never runs as libtest (`test = false`, `required-features` unmet under cargo defaults, or `harness = false`) no longer earns `exposed`; its changed behavior reads `no_static_path`. A target registered as a custom harness keeps its file, and an unavailable `cargo metadata` keeps the previous credit (RIPR-SPEC-0153, #8016).
