<!-- section: Fixed -->
- TypeScript related-test rows now show the assertion relevant to the changed
  behavior instead of the strongest assertion in the test (RIPR-SPEC-0224).
  The row reads the same `ts_oracle_kind_matches_seam` rule the classifier
  already uses, so a changed `return` is shown with its value assertion
  rather than a stronger `.toThrow(DiscountError)`, and a changed `throw`
  with its error assertion rather than a stronger `.toBe(...)`. When every
  assertion in a test observes another behavior family, the row shows no
  oracle instead of the wrong-family one. Equal candidates are chosen by the
  later source line, then their own text, so the order the assertions are
  listed in does not change the row. Exposure classes and the strongest-oracle summary are
  unchanged. A finding whose row strength moved this way names it as
  `typescript_assertion_selection` evidence and never emits a repair packet
  (#5525).
