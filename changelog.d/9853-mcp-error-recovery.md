<!-- section: Fixed -->
- MCP `tools/call` argument-validation errors (`-32602`) now carry structured
  recovery data — the offending `argument`, the `allowed` argument names, or
  the `expected` type — instead of `data: null`, so an agent can branch on the
  payload instead of parsing message prose (#9853).
