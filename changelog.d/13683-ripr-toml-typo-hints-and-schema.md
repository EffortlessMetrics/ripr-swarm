<!-- section: Added -->
- A misspelled `ripr.toml` key now names the nearest valid key in the same
  table (``did you mean `mode`?``), and `schemas/config/ripr-toml.schema.json`
  lets editors validate and complete `ripr.toml`. `docs/CONFIGURATION.md`
  lists every key, and a test keeps the parser, schema and docs in step (#13682).
