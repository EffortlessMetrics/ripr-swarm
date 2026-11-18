<!-- section: Fixed -->
- Rust: a test that hands an item declaration of the owner's name to a macro
  defined outside the file, at statement position
  (`emit!(fn suit_color(..) { .. });` inside the test body), no longer reads
  `exposed` for a call of that name, because the unread expansion may put its
  own item in the body. A call or bare mention in the same position is
  unaffected. `stringify!`, `concat!`, `include_str!`, `include_bytes!`,
  `env!`, `option_env!`, `cfg!`, `line!`, `file!`, `column!` and
  `module_path!` are now read as defining no item (#8467).
