<!-- section: Fixed -->
- The TypeScript committed-snapshot counter now skips `//` line comments
  between a matcher name and its `(`, and counts a matcher name used as a
  value (parenthesised, cast, `satisfies`, aliased, passed or destructured)
  as two calls, so such a test no longer gets strong credit from a missed
  second call (#8895).
