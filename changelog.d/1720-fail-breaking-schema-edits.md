<!-- section: Internal / CI -->
- `cargo xtask check-verification-contracts` now fails when a published
  `schemas/ripr/*.json` changes in a breaking way (removed property or enum
  value, newly required field, narrowed type, tightened bound) relative to the
  merge base without bumping its `schema_version`, so JSON output stability no
  longer depends on reviewers noticing a schema edit (#1720, #13689).
