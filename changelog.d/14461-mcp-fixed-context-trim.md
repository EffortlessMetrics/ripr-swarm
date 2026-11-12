<!-- section: Changed -->
- `ripr mcp --stdio` sends less fixed context per session: the
  instructions name each route once, read-tool descriptions no longer
  restate the read-only boundary the annotations and instructions already
  carry, and `ripr_list_gaps`/`ripr_refresh` argument descriptions drop
  implementation detail (about 240 fewer estimated tokens before the first
  call). Contract wording, failure codes and tools are unchanged (#14461).
