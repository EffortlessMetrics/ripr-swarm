- MCP now rejects explicit `null` for optional snapshot and paging arguments
  with correlated Invalid Params errors, matching the advertised input types;
  omission still uses the documented defaults (#7314).
