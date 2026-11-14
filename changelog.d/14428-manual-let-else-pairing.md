<!-- section: Fixed -->
- `ripr check` pairs a removed `None => return R,` match arm with the added
  `return R;` of a `let ... else` rewrite (clippy `manual_let_else`) when the
  scrutinee is the same on both sides, so a behavior-preserving rewrite no
  longer headlines a repairable `return_value` gap; a changed scrutinee or a
  changed return value keeps both probes (RIPR-SPEC-0001, #14428).
