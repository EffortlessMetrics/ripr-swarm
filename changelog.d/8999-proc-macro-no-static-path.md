<!-- section: Fixed -->
- A finding in a proc-macro crate (a package whose `[lib]` sets
  `proc-macro = true`, judged package-wide from the manifest) that would read
  `no_static_path` is now `static_unknown` with the unknown class's
  `static_probe_unknown` stop reason and an evidence note that tests reach the
  code through derive or attribute expansion, so serde_derive-style changes no
  longer show a WARNING-level "no test reaches this" claim. Ordinary library
  crates, including ones that merely mention `proc_macro` in comments or
  strings, are unchanged (#8999).
