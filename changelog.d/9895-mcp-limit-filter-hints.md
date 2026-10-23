<!-- section: Fixed -->
- MCP `offset` and `limit` validation errors now name the real constraint and a
  recovery: an integer past `u64::MAX` or a `1e30` limit reports the largest
  accepted integer (256 for `ripr_list_repair_attempts`) instead of the wrong
  "must be a non-negative integer", and negative, fractional and non-numeric
  values each get their own message plus `minimum`, `maximum` and `recovery`
  in the error data (#9895).
