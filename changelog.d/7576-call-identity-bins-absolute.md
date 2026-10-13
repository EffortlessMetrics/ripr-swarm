<!-- section: Fixed -->
- Same-name call identity now reads binary crates correctly. A `src/` module
  that only `src/main.rs` declares composes into the binary, not the library,
  so a test there calling `crate::render` is no longer credited to the
  library's `render`. `src/bin/<name>/main.rs` is its crate's root, and an
  integration test never names a binary's crate. A module that both roots
  declare, or neither, or any module when a root has an item-position macro,
  switches the rule off for library and binary owners; a macro inside
  `fn main` does not. Two packages under an absolute path without a
  `crates/` directory stay apart, and an index that mixes absolute and
  relative paths switches the rule off. A package with only `src/main.rs`
  composes its `src/` modules into the binary. Every gap either switches the
  rule off or leaves the call unsettled, so it keeps name-only matching and
  never grants new credit (#7576).
