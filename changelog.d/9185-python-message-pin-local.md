<!-- section: Fixed -->
- A Python message-only change to a local message that the owner raises through
  (`msg = "..."` then `raise Exc(msg)`, the attrs `default_if_none` shape) no
  longer reads `exposed` when every strong related error assertion pins the
  exception with a literal `match=` or `assertRaisesRegex` pattern that matches
  the old and the new message alike. The finding reports `weakly_exposed`
  naming the changed message; a name used anywhere besides the assignment and
  one `raise Exc(name)` keeps the existing verdict (#9185).
