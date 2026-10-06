<!-- section: Fixed -->
- Repository inventory keeps one `error_variant` seam per error constructor.
  The `return` around `return Err(X)` and the payload call inside
  `Err(Error::X(..))` no longer add twin seams, so pilot no longer spends two
  picks on one error. Diff findings are unchanged. Classified seam caches move
  to 1.42 / 0.48, so warm entries rebuild (#6914).
