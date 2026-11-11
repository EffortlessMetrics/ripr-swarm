<!-- section: Fixed -->
- A file declared by an out-of-line `#[cfg(kani)] mod proofs;` (resolved
  through the parent's declaration), and any file nested below it, is now
  verification source: a changed helper there seeds no production probe and is
  disclosed as `verification_only_change`, as the body of an inline
  `#[cfg(kani)] mod` already was. `cfg(not(kani))`, `cfg(any(kani, ..))`,
  exclusive `cfg(kani)`/`cfg(not(kani))` alternatives, a second owner of the
  file and an ungated module named `kani_proofs` keep their production probes
  (#13736).
