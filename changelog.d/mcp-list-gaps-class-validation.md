<!-- section: Fixed -->
- `ripr_list_gaps` over `ripr mcp --stdio` now rejects a `class` filter that is
  not an exposure-class wire token (for example `weak`) with `-32602` naming
  the allowed tokens, instead of returning an empty page that read like "no
  matching gaps" (#2379).
