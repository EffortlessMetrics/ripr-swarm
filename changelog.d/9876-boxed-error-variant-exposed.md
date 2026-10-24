<!-- section: Fixed -->
- A changed `return Err(Box::new(E::V))` in a `Box<dyn Error>` owner is read as
  the variant `E::V`, so a test that downcasts to the exact variant now
  exposes it and a sibling-variant downcast still does not (#9876).
