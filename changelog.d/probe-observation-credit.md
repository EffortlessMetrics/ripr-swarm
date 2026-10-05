<!-- section: Fixed -->
- A changed `let` line with a type annotation or pattern
  (`let value: u8 = parse()?;`) no longer acquires a spurious
  `field_construction` probe; only a struct literal in the initializer reads as
  field construction (#6676).
- An exact `assert_eq!` pin on the owner's own call now confirms a returned
  bitwise-OR expression (`u16::from(lo) | (u16::from(hi) << 8)`) as it does
  for arithmetic; closures and lazy `||` stay refused. An operand-swap rewrite
  no longer adds a second `static_unknown` finding on the removed line (#6675).
- A field set in a hand-written `impl Clone` is now credited when a test
  checks `assert_eq!(value.clone(), value)` and the type's `PartialEq` is
  derived, with a field type that compares by value. A hand-written
  `PartialEq`, `assert_ne!` or a comparison with another value still gets no
  credit (#6692).
