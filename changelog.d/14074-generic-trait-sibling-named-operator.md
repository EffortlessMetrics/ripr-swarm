<!-- section: Fixed -->
- An operator trait method called by name no longer credits a sibling impl
  it cannot select. With `impl Add<u32> for Meters` beside
  `impl Add<Meters> for Meters`, a change to the `Add<Meters>` impl no longer
  reads `exposed` from a test whose only use is `Meters(1).add(2u32)`. A test
  that also uses the operator token (`+`, `-`, `*`, ...), or `sum` / `product`
  for `add` / `mul`, keeps its relation, since that syntax may run the changed
  impl unnamed (#14074).
