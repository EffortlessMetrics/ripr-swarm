<!-- section: Fixed -->
- A terminal `if outcome != expected { return Err(..) }` guard, or an
  `assert!(outcome == expected)` equality, whose expected value is bound only
  to an unfilled placeholder macro (`todo!()`, `unimplemented!()`) no longer
  reads as exposing the changed behavior, so `ripr check` keeps reporting the
  gap until the value is filled in; a guard or equality on a real value keeps
  its credit (#14882).
