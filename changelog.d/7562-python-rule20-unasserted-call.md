<!-- section: Fixed -->
- Python rule 20 ignores a discarded owner call only when the changed
  lines are `return`s of a value expression with no recognized effect
  store. A strong related test that only asserts inputs where a removed
  additive term is 0 then reads `weakly_exposed` even if the same test
  also contains such a discarded call whose term is nonzero.
  Assigned-but-unread, nested, or otherwise unclear calls still block
  the downgrade
  (RIPR-SPEC-0238, #7562).
