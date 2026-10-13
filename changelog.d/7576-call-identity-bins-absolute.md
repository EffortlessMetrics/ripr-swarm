<!-- section: Fixed -->
- Same-name call identity now reads binary crates correctly. A `src/` module
  that only `src/main.rs` declares composes into the binary, not the library,
  so a test there calling `crate::render` is no longer credited to the
  library's `render`. `src/bin/<name>/main.rs` is its crate's root. A module
  that both roots declare, or neither, switches the rule off for its
  definitions. Two packages under an absolute path without a `crates/`
  directory stay apart, and an index that mixes absolute and relative paths
  switches the rule off. Every gap falls back to name-only matching and never
  grants new credit (#7576).
