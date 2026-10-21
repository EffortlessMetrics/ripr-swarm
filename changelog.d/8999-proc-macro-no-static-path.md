<!-- section: Fixed -->
- A finding in the library of a proc-macro package (`[lib] proc-macro = true`,
  read from the committed `Cargo.toml`) that would read `no_static_path` is now
  `static_unknown` with the unknown class's `static_probe_unknown` stop
  reason, unknown reach and later stages, and an evidence note that tests reach
  the code through derive or attribute expansion; a transitive or macro
  witness pointer is kept. Build scripts, bins and integration tests of the
  package, and ordinary crates that merely mention `proc_macro` in comments or
  strings, are unchanged (#8999).
