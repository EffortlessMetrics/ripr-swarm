<!-- section: Added -->
- The Rust verdict corpus gains five authored cases from `ripr pilot` picks
  that real cargo-mutants runs refuted: a boundary on a pure query call or a
  counted local (#6693), an `ok_or` error returned through `?` (#6695), a
  hand-written `Clone` checked by whole-value equality (#6692), and a generic
  helper reached only through its wrapper (#6694). Every listed mutant fails
  the stored crate's tests; ripr currently reads four as gaps and abstains on
  one.
