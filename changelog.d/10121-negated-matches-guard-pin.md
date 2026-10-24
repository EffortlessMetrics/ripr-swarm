<!-- section: Fixed -->
- A terminal Err-return guard over a negated literal `matches!`
  (`if !matches!(owner(..), 2) { return Err(..) }`) now pins the owner's
  value like its `assert!(matches!(..))` twin, so a changed return value it
  discriminates reads `exposed` instead of `weakly_exposed` with an
  unverified observation (#10121).
