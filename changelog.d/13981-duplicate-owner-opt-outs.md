<!-- section: Fixed -->
- `cargo test -p ripr --lib` compiles again: remove the duplicated
  `owner_opt_outs` field left in one `human.rs` test expectation by the
  incomplete #13967 repair (#13981).
