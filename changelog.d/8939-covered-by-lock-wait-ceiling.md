<!-- section: Fixed -->
- The `check-file-policy` covered_by cargo lock-wait retry now also has a
  90-minute total wall-clock ceiling, summed from the recorded attempt
  durations, so a lock that never clears no longer holds a run for hours. Hitting
  the ceiling stops retrying and reports the same distinct
  `covered_by_lock_wait_timeout` instrument failure, naming the ceiling. A real
  non-lock timeout is unchanged. A new end-to-end test drives the `precommit-v2`
  composition with a deferred covered_by instrument failure and pins that it is
  reported as `infrastructure_failure` after the diff checks and selected Clippy
  run, while a real policy failure alongside it is never classified as
  infrastructure (#8939).
