<!-- section: Added -->
- `ripr rerun --json` now has a published schema,
  `schemas/ripr/rerun.schema.json`, tested against the real command's output
  for both selectors, `--before` movement and `--check-parity`. The emitted
  bytes and `schema_version` `ripr-targeted-rerun-v1` are unchanged (#3920).
