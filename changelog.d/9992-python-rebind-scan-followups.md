<!-- section: Fixed -->
- A Python protocol-entry receiver rebound by `(r) += ...`, `(r): list = ...`,
  a comment inside the brackets, a backslash continuation before the `=`,
  `for (r)in ...`, a run of blanks after `for`/`as`/`def`/`class`, or a header
  holding a `'''it's'''` string is no longer credited `exposed` through
  `protocol_entry_call`. A quoted or commented `for` no longer reads a
  membership test as a rebind, so `assert "for" and ((r)) in [r]` keeps its
  credit (#9992, #9207). A header split across lines is still not read, so a
  one-line-suite rebind after it keeps the credit.
