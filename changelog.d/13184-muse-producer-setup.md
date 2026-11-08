<!-- section: Internal / CI -->
- `cargo xtask muse-plugin-setup` produces deterministic typed native Muse
  setup trials for the muse-plugin suite: it materializes a private fixture
  copy, resolves `ripr` explicitly against a controlled PATH, runs bounded
  setup steps (`muse-version`, `ripr-version`, `ripr-mcp-probe`,
  `plugins-validate`, `plugins-install`, `plugins-inspect`), binds every
  emitted byte to a sha256 manifest, and re-derives an evaluator record that
  refuses dishonest credit for the `missing-or-unapproved-server` scenario
  (#13184).
