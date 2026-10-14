<!-- section: Fixed -->
- Match-arm selection reads integer range patterns (`1..=9 =>`) by value and
  credits an input that moved between a changed pattern and a later arm with a
  provably unequal result (a literal, or a variant of literals), so widening
  `1..=8` to `1..=9` under a test that pins `f(9)` reads `exposed` instead of a
  false `weakly_exposed` gap (RIPR-SPEC-0229 example 8; #5992).
