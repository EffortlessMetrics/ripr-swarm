<!-- section: Fixed -->
- Assertion dependency identity uses Cargo-compatible home discovery when
  `HOME` or `CARGO_HOME` is absent or `CARGO_HOME` is empty, retaining config
  invalidation and refusal controls. Canonical fixture dependencies still need
  explicit preparation in the selected Cargo home (#7920).
