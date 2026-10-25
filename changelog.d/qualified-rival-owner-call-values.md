<!-- section: Fixed -->
- A module-qualified call of a same-named rival function (`rival::thr(101)`)
  no longer supplies a boundary-equal input to the changed owner, so it cannot
  suppress the missing-equality gap of a returned comparison; qualified calls
  that resolve to the owner's own module (`crate::pricing::thr(..)`) still
  count (#9218).
