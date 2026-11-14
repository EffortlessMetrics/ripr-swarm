<!-- section: Added -->
- `ripr capabilities --json` now has a published schema,
  `schemas/ripr/capabilities.schema.json`, so a consumer can validate the
  language capability registry instead of reading its shape from prose. The
  emitted bytes and `schema_version` `0.1` are unchanged; tests validate what
  the real command writes, for the whole table and every `--language`
  projection, against that file (#14067).
