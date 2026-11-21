<!-- section: Security -->
- Security: `ripr lsp` no longer reads a client-named path without a bound
  when it compares a changed buffer with the file on disk. A `didChange`
  for `/dev/zero`, a directory or an oversized file now goes through the
  same regular-file reader capped at 16 MiB, instead of growing memory
  without limit (#11507).
- Report commands now refuse a relative output path, resolved against the
  current directory, when a committed `target` link there points outside
  it, and `ripr evidence-health` checks containment before analysis writes
  its cache under `target/ripr/cache`. Before, a committed
  `target -> /elsewhere` link redirected those writes (#11526, #11477).
- `ripr receipt write`, `ripr feedback record` and `ripr feedback export`
  no longer skip the directory-link check on their `target/ripr` outputs
  when `RIPR_CACHE_DIR` is set, and `ripr receipt write` without `--root`
  checks the current directory, so a committed directory link cannot
  redirect those writes out of the workspace (#11614, #11477).
