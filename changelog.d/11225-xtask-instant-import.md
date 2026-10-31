<!-- section: Fixed -->
- The xtask process tests no longer break Windows `clippy -D warnings` with an
  unused `Instant` import; the import is gated to the Unix tests that use it
  (#11225).
