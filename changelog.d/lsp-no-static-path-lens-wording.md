<!-- section: Fixed -->
- The `ripr lsp` code lens on a `no_static_path` finding now reads "no static
  test path found (no path)" instead of "no test reaches this change", so it no
  longer claims a missing test where `ripr check` reports a static-analysis
  limitation such as a transitive helper path (RIPR-SPEC-0100, #15432).
