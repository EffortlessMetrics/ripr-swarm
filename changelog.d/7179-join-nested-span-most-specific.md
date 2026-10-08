<!-- section: Fixed -->
- The shared seam-finding join now binds a finding inside nested same-kind
  spans to the inner (most specific) seam only: the containing seam no
  longer credits the inner finding's witness on the CLI and editor cards,
  and the valid inner bind is no longer lost to the MCP fan-in refusal
  (#7179).
