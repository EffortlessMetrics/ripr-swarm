<!-- section: Fixed -->
- A Python `self` field or module-object field write by an owner whose body
  holds no `return` or `yield` is no longer credited `exposed` through the
  owner-result branches (for example `assert_equal(rec.last, rec.note("x") or
  None)` or `assert rec.note("x") is None`). The call hands back only the
  implicit `None`, so only an assertion that reads the field after the
  completed call earns credit. An owner with any `return` keeps its prior
  reading (#10178).
