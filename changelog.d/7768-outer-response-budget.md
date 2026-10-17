<!-- section: Fixed -->
- MCP tool and resource responses are measured against the response bound
  minus the request ID's JSON-serialized bytes and a fixed outer reserve, so an
  over-bound result returns the typed `result_too_large` refusal with recovery
  instead of the correlated generic `-32603` fallback (#7768).
