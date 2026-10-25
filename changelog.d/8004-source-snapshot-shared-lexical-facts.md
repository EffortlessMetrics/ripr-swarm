<!-- section: Internal / CI -->
- Complete-source declaration, lexical-scope, item-nesting and module-shadow
  facts are computed once per immutable source snapshot and shared across
  owners, executable callers and assertion operands, replacing per-owner
  reparse caches. Ambiguous or unparsed declarations stay unknown, and a
  same-named function pair on one line no longer borrows the first
  declaration's name scope (#8004).
