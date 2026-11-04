<!-- section: Added -->
- `ripr.toml` accepts optional `[[overrides]]` rows so one workspace crate
  (Cargo package name or path glob) can override `[severity]` and `[oracles]`
  without changing the repo-wide default. Most specific match wins; unknown
  crate names fail closed. Existing configs without the section are unchanged
  (#2671).
