<!-- section: Fixed -->
- Rust related tests compare module identity instead of the bare file stem, so
  a change in `serialize/mod.rs` no longer cites the inline tests of
  `tokenizer/mod.rs`, and two crates' `lib.rs` or `main.rs` no longer pair.
  On the html5ever corpus pin the finding now names the dependent crate's
  public-API test as a limitation instead of five unrelated tokenizer tests
  (#5395).
