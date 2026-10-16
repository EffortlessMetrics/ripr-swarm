<!-- section: Fixed -->
- Tests in a `[[test]]` target that `cargo test` never runs no longer count as
  evidence. A target with `test = false`, with `required-features` outside the
  default feature set, or with `harness = false` and no registered custom
  harness previously read `exposed`; its findings now read `no_static_path`.
  Unavailable `cargo metadata` keeps the old credit (#8016).
