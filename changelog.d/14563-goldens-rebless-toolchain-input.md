<!-- section: Internal / CI -->
- `.github/workflows/goldens-rebless.yml` passes `toolchain: 1.95.0` to its
  `dtolnay/rust-toolchain` step, and a contract test asserts that exact
  `with:` input on the bless step (#14563).
