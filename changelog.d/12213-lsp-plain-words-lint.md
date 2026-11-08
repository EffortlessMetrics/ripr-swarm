<!-- section: Internal / CI -->
- `cargo xtask check-user-text` now also scans the string literals in the
  non-test LSP Rust sources (`crates/ripr/src/lsp.rs` and `crates/ripr/src/lsp/`)
  as the `lsp-source` surface (#12213). `seam` and `discriminator` are held at a
  ceiling of 0 there; `RIPR-SPEC-`, `probe` and snake_case state tokens stay at
  their recorded ceilings. Text built outside those literals (for example
  producer summaries reworded at runtime) is not covered by this scan.
