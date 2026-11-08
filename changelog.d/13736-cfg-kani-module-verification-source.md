<!-- section: Fixed -->
- A helper function inside a module gated by an exact `#[cfg(kani)]` (out-of-line
  `#[cfg(kani)] mod proofs;` resolved through its parent declaration, nested
  files below it, or an inline module) is now verification source: a change to
  it seeds no production probe and is disclosed as `verification_only_change`
  instead of producing production findings. `cfg(not(kani))`,
  `cfg(any(kani, ..))` and an ungated module named `kani_proofs` keep their
  production probes (#13736).
