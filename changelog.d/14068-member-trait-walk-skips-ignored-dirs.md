<!-- section: Fixed -->
- Rust: a `trait` declared only under a workspace member's `fixtures/` or
  `node_modules/` directory no longer counts as a member declaration. A
  compile-fail fixture declaring `trait Display` could turn a changed
  `std::fmt::Display` impl from the conservative reading into `no_static_path`.
  Those files are not analysis subjects and are outside the classified cache
  key, so the walk no longer reads them (#14068).
