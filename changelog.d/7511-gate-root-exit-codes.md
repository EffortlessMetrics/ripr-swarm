<!-- section: Docs -->
- `docs/EXIT_CODES.md` now names that `ripr gate evaluate --root` missing
  or not a directory is argv-level exit 2, names the root, and writes no
  `gate-decision` report, distinct from a completed `config_error` that
  still writes the report (#7511).
