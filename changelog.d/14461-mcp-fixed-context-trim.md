<!-- section: Changed -->
- `ripr mcp --stdio` sends less fixed context per session: the
  instructions name each route once, read-tool descriptions no longer
  restate the read-only boundary the annotations and instructions already
  carry, and `ripr_list_gaps`/`ripr_refresh` argument descriptions drop
  implementation detail (about 240 fewer estimated tokens before the first
  call). Tools, failure codes and pinned contract wording are unchanged (#14461).
