<!-- section: Fixed -->
- The `[languages] enabled` list that `ripr check` and `ripr doctor` print when
  a preview language is off keeps `rust` in a mixed repository whose crate sits
  below the root (`backend/Cargo.toml`, `crates/core/Cargo.toml`). Before, the
  list named only the preview languages, and pasting it switched off the Rust
  analysis the same run had just produced (#15914).
