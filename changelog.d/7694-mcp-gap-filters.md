<!-- section: Changed -->
- `ripr_list_gaps` narrows stored gaps by file, class, language, and repair
  readiness before paging, with truthful counts and bound continuations.
  Gap-list envelopes move from `ripr-mcp-gap-list-v1` to
  `ripr-mcp-gap-list-v2` because the strict v1 schema rejects the added fields;
  strict readers must rediscover the tool schema and accept v2 (#7464).
