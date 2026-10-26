<!-- section: Fixed -->
- A Python protocol-entry receiver rebound by `(r) += ...`, `(r): list = ...`,
  `if c: r += ...`, a `case` pattern capture (`case (r):`, `case {"k": r}:`),
  `import r`, `nonlocal r`, a comment or backslash continuation
  inside or before a bracketed target, `for (r)in ...`, a run of blanks after
  `for`/`as`/`def`/`class`, a split walrus, or a header holding a `'''it's'''`
  string is no longer credited `exposed` through `protocol_entry_call`. A quoted
  or commented `for` no longer reads a membership test as a rebind, so
  `assert "for" and ((r)) in [r]` keeps its credit (#9992, #9207). Known limits
  remain: a multi-line tuple target such as `(x,\n r) = ...` or a header split
  across lines is not read, so the receiver keeps its credit, and a dict key at
  the start of a continuation line withholds credit.
