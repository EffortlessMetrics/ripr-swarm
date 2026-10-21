<!-- section: Fixed -->
- A changed `impl` header line (`impl Tr for K where Self: Sized {`) is no longer
  read as a constructed field. `Self: Sized` looked like a field `name: Type`,
  so the header was reported as a `field_construction` probe with a
  `no_static_path` gap that does not exist. The header is now one
  `static_unknown` probe, so a changed bound is still disclosed, and the `fn`
  bodies inside the `impl` keep their own probes (#8389).
