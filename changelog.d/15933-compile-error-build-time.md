<!-- section: Fixed -->
- `ripr check` no longer reports a changed `compile_error!(..)` as a deleted
  call and a log side effect with no test path. The invocation stops the
  build, so it keeps one `static_unknown` finding with the
  `compile_time_invariant` stop reason, on one line or split over several
  (#15933).
