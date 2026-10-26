<!-- section: Fixed -->
- Rust analysis keeps qualified receiver initializers and type annotations
  unresolved when competing methods share only a compact type name, so a test
  of another crate's type cannot supply direct reach through that spelling
  (#8782).
