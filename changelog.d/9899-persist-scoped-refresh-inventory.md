<!-- section: Changed -->
- `ripr_refresh` on a code diff answers in a new `ripr mcp --stdio`
  process with a warm on-disk cache in about 24 s instead of about 60 s on
  this repository: the classified seams a refresh binds to its
  findings are now persisted under the cache root and served only for the
  exact workspace, configuration and changed-scope they were computed for, as
  a second refresh in one process already was. Findings and refresh output are
  unchanged (#9899).
