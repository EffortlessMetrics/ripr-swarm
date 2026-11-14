<!-- section: Changed -->
- `ripr lsp` keeps unchanged files' parse facts in memory between saves
  instead of re-reading and decoding every on-disk cache entry, cutting about
  3 s from a warm save on a ~1,700-file workspace (#13754).
