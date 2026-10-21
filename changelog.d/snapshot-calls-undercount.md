<!-- section: Fixed -->
- The TypeScript committed-snapshot counter now skips `//` line comments
  (ending at LF, CR, U+2028 or U+2029) between a matcher name and its `(`,
  and counts a matcher-shaped name used as a value (parenthesised, cast,
  `satisfies`, aliased, passed, destructured or `.bind`) as two calls, so
  such a test no longer gets strong credit from a missed second call
  (#8895).
