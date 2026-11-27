<!-- section: Internal / CI -->
- The required Rust run lints the Rust-only `ripr` library
  (`--no-default-features --features lang-rust`) with warnings denied, in
  shard 2 of the hosted test shards or in the gates job when it runs the
  tests, so code used only by a preview language cannot leave the supported
  Rust-only build full of warnings again (#14263).
