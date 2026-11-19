<!-- section: Fixed -->
- `ripr repair` (and the refusal text shared with MCP) now prints
  `ripr agent stub --seam-id ID` instead of `--at FILE:LINE --kind K` when the
  line holds several seams of that kind (`Config { retries: 5, verbose: false }`),
  so the suggested next command runs instead of refusing as ambiguous and names
  the changed seam (#7762).
