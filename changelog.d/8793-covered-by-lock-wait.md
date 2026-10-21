<!-- section: Fixed -->
- `cargo xtask precommit` no longer fails `check-file-policy` when the covered_by
  enumeration only timed out waiting on a cargo lock held by another build: such
  a spawn is retried (bounded), a persistent lock wait is reported as
  `covered_by_lock_wait_timeout` (infrastructure, not a policy violation), and an
  instrument failure no longer hides the remaining precommit checks, though it
  still fails the run (#8793).
