<!-- section: Fixed -->
- A Python message-only edit to the string literal of a multi-line
  `raise Exc(\n    "message"\n)` no longer reads `exposed` when every strong
  related error assertion pins the exception with a literal `match=` pattern
  that fits the old and the new message alike. The finding reports
  `weakly_exposed` naming the changed message. F-strings, bytes literals,
  implicit concatenation, keyword arguments, non-`raise` openings and unreadable
  patterns keep the existing verdict (#9185).
