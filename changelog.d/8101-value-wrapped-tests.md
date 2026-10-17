<!-- section: Fixed -->
- A test held in a value wrapper (`Some(..)`, `Ok::<bool, ()>(..)`, a
  constructor, `Box::new(..)`, `Wrapper::from(..)`, `[..]`, `[..; n]`,
  `vec![..]`, or `.unwrap()`, `.expect(..)`, `.clone()` or `.to_owned()` on
  one operand) and compared with a non-literal or with `false`
  (`assert_eq!(Some(r == Err(E::Bad)), Some(expected))`) no longer earns
  `exact_value`/strong credit, nor does one compared with a different
  wrapper or a bare literal, held in an empty or unknown-length repeat, or
  paired with `false` in a list; the same test asserted true through
  `Some(true)` still pins (RIPR-SPEC-0231 rule 1; #8101).
