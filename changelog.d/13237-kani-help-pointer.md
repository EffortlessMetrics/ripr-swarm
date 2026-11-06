<!-- section: Fixed -->
- Route every hand-written `ripr reports kani` argument error (missing,
  duplicate, empty, and conflicting-path refusals) through the shared
  `suggest::help_pointer` owner, so each names `ripr reports kani --help` — the
  page that documents the flagged options — instead of `ripr help`, the
  top-level overview that documents none of them. The command no longer
  contradicts its own unknown-flag error, which already named the precise
  pointer (#13237).
