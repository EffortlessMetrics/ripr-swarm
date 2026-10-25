<!-- section: Fixed -->
- A Rust owner whose own body is the only place naming it, such as
  `fn get(&self) -> u32 { self.inner.get() }` (a call of `Cell::get`), no longer
  keeps weak reach from a same-file test that does not call it: the body cannot
  run before the owner is entered, and the entry spells the name elsewhere, so
  with nothing else naming it the finding reads `no_static_path`, as the same
  owner renamed to `read` already did. Any other naming (a caller, a same-named
  call on another receiver in a different function, a doc, a macro) still keeps
  reach undecided, and an owner without parser facts keeps the old reading
  (#10160).
