<!-- section: Fixed -->
- One error constructor produces one `error_variant` seam. `return Err(X)` no
  longer adds a twin spanning the whole `return`, and `Err(Error::X(..))` no
  longer adds a twin for the inner `Error::X(..)` call. Pilot no longer spends
  two top picks on one line. Caches move to 1.41 / 0.47 / file facts 1.31, so
  warm entries rebuild (#6914).
