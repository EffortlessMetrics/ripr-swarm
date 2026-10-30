<!-- section: Fixed -->
- `ripr repair` without an item no longer reads as a verdict on your change
  when it refuses because some other seam's tests already grip it: that
  reason now names the seam it is about (file, line and code), and when the
  diff a bare `ripr check` reads is available, gripped seams outside the
  changed lines are counted instead of cited. A bool function whose whole
  body is the changed comparison now reads as gripped on its return-value
  seam too when a boundary `assert!(f(x))` or `assert!(!f(x))` check is
  statically consistent, as its predicate seam already did, so `ripr repair` no longer starts a repair on a change `ripr
  check` reads as exposed (#11111).
