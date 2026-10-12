<!-- section: Changed -->
- A warm `ripr check` on a large workspace is faster again: parser-backed
  file facts now store each file's compact module item scopes, so same-file
  helper crediting no longer reparses test files on cache hits, and a helper
  body's assertions are parsed once per run instead of once per calling test.
  The file-fact cache schema moves to `1.32`, so the first run after upgrade
  rebuilds that cache. On ripr-swarm (pin 7c64b9c) the warm check drops from
  about 6 s to about 4.4 s, and its JSON is byte-identical to main's (#5363).
