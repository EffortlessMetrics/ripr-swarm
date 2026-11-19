<!-- section: Fixed -->
- A literal whose exact type is left to inference no longer credits a sibling
  impl of a generic trait it cannot select. With `impl From<u32> for Cents`
  beside `impl From<&str> for Cents`, a change to the `&str` impl no longer
  reads `exposed` from a test that calls `Cents::from(2)`, `Cents::from(0x2)`
  or passes a binding from `let n = 2;` or `let n = 2u32;`: an integer literal
  is never `&str`. The same holds for float literals against integer impls
  and string literals against numeric ones. A literal whose kind fits the
  changed impl keeps its relation (#14074).
