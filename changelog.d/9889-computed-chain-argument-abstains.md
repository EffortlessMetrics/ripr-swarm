<!-- section: Fixed -->
- A return-value, match-arm or field probe in a helper reached only through a
  wrapper chain now reads `infection_unknown`, not a `weakly_exposed` gap, when
  the wrapper hands the helper an argument ripr cannot bind to an exact value
  (`wide_x(input.trim())`), matching its predicate sibling (#9889).
