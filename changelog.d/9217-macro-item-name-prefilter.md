<!-- section: Changed -->
- `ripr check` skips the per-callee macro-item parse when the test source has
  no `!`, or when the callee name never appears and the file has no
  `macro_rules!`. Classification answers stay the same; an unparsable file
  that holds `macro_rules!` remains undecided (#9217).
