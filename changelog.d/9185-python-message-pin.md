<!-- section: Fixed -->
- A Python message-only change to a `raise` no longer reads `exposed` when every
  strong related error assertion pins the exception with a literal `match=` or
  `assertRaisesRegex` pattern that matches the old and the new message alike.
  The finding reports `weakly_exposed` naming the changed message; patterns
  with regex syntax, non-literal patterns and whole-message comparisons keep
  the existing verdict (#9185).
