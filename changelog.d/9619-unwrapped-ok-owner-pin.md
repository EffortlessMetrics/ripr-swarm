<!-- section: Fixed -->
- An owner whose changed `Ok(..)`/`Some(..)` tail sits beside `Err`/`None`
  exits is now credited when a test compares `owner(..).unwrap()` or
  `.expect("..")` with the literal, directly or through a once-bound local;
  mutable, shadowed, derived and non-unwrap readings keep the conservative
  verdict (#9619).
