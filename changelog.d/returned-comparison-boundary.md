<!-- section: Fixed -->
- A Rust `return_value` finding whose changed expression is one bare comparison
  (`x > 100` changed to `x > 101`) no longer reads `exposed` when no related
  test input separates the old and new threshold. It now takes the same
  missing equality-boundary discriminator and infection reading as the
  `predicate` finding on that line, so it reads `weakly_exposed` instead of
  outranking the boundary gap. Compound and non-comparison return values keep
  their reading (#9094).
