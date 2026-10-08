<!-- section: Fixed -->
- Repository inventory keeps one `error_variant` seam per error constructor.
  The `return` around `return Err(X)` and the payload call inside
  `Err(Error::X(..))` no longer add twin seams, so pilot no longer spends two
  picks on one error. Diff findings are unchanged. Classified seam caches move
  to 1.43 / 0.49, so warm entries rebuild. Saved baselines or receipts that
  named a dropped `return` or payload seam will no longer find it (#6914).
