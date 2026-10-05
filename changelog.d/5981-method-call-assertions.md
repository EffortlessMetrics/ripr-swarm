<!-- section: Fixed -->
- Suggested assertions (`ripr pilot`, agent seam packets, briefs and editor
  actions) call the owner the way the parser shows it is declared: a method
  with a `self` receiver reads `/* ByteSize value */.as_whole_units(..)`, an
  associated function reads `ByteSize::name(..)`, and a module-level function
  keeps `name(..)`. When the receiver or path is not established (trait
  default methods, function-local `fn`s, blanket impls, lexical fallback) the
  template names the owner in a comment instead of presenting a free call
  that would not compile. Observed values whose path spelling establishes a
  constant (`u64::MAX`, `crate::KIB`) carry the new
  `constant` value context instead of `enum_variant`; ambiguous all-caps
  paths such as `Kind::ON` or `Limits::MAX_LEN` keep `enum_variant`. Evidence-health
  `observed_value_context_counts` gains a `constant` bucket, and the
  classified-seam cache generations move to `1.33` / `0.39` (#5357).
