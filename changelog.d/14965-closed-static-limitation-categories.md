<!-- section: Internal / CI -->
- The evidence record's stage static-limitation category producer now
  returns either a typed `StaticLimitKind` token or a named untyped renderer
  category, so it cannot emit a new category as a bare string. A
  guard test keeps the untyped tokens from shadowing any typed token, and
  the `dynamic_dispatch` category now comes from `StaticLimitKind`. Wire
  strings are unchanged. The untyped categories are still read from stage
  prose until typed producers replace them (#14965).
