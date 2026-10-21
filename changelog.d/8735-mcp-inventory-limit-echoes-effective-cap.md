<!-- section: Fixed -->
- `ripr mcp --stdio` `ripr_list_repair_attempts` no longer echoes an unclamped request in the typed page: the input schema now declares `maximum: 256` on `limit`, a request above the 256-row cap is rejected as invalid params at the dispatch edge, and `page.limit` plus the bound continuation echo the effective limit that was actually served (#8735).
