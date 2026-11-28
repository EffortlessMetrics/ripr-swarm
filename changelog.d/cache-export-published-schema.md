<!-- section: Added -->
- `ripr cache export` now has a published schema for its success report,
  `schemas/ripr/cache-export.schema.json`, tested against the real command's
  output, including that its file and byte counts match the exported
  directory. The emitted bytes and `schema_version` `0.1` are unchanged (#3920).
