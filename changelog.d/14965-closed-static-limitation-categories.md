<!-- section: Internal / CI -->
- The evidence record's stage static-limitation categories are now a closed
  set: each is either a typed `StaticLimitKind` token or a named untyped
  renderer category, so a new category cannot appear as a bare string. A
  guard test keeps the untyped tokens from shadowing any typed token, and
  the `dynamic_dispatch` category now comes from `StaticLimitKind`. Wire
  strings are unchanged. The untyped categories are still read from stage
  prose until typed producers replace them (#14965).
