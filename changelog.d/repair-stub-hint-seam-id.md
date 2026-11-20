<!-- section: Fixed -->
- `ripr repair` (and the CLI refusal text it shares) now prints
  `ripr agent stub --seam-id ID` instead of `--at FILE:LINE --kind K` when the
  line holds several seams of that kind (`Config { retries: 5, verbose: false }`),
  so the suggested next command runs instead of refusing as ambiguous and names
  the changed seam (#7762).
The MCP `ripr_prepare_repair` refusal's `stub_route` command names `--seam-id` under the same ambiguity.
