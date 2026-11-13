<!-- section: Internal / CI -->
- `.github/workflows/goldens-rebless.yml` passes `toolchain: 1.95.0` to its
  `dtolnay/rust-toolchain` step, and a contract test rejects any workflow that
  pins the no-default `master`/`v1` ref without an explicit input (#14563).
