<!-- section: Fixed -->
- A test held in a struct literal (`Foo { ok: x == 1 }`), at the end of a
  statement block, or behind a field or method on a wrapper
  (`Wrapper(x == 1).0`, `Some(x == 3).unwrap_or(false)`) and compared with a
  non-literal or `false` no longer earns exact/strong credit; a plain field
  read compared with `true` still pins. `Box::<bool>::new(..)`,
  `Option::Some(..)` and `Result::<bool, ()>::Ok(..)` now read as the
  wrappers they spell, and a trailing comma no longer changes a list's length
  (RIPR-SPEC-0231 rule 1; #8615).
