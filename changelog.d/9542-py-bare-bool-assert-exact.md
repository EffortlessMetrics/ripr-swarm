<!-- section: Internal / CI -->
- Python: a bare `assert f(lit, ..)` or `assert not f(lit, ..)` now counts as
  an exact pin when its single resolved workspace callee provably returns a
  `bool` (a `-> bool` annotation, or a lone comparison, `not` or boolean-literal
  return). The three py-grid exact rows (boundary, equality, logical) move from
  `weakly_exposed` to `exposed` and leave the false-verdict list (#9542).
