<!-- section: Internal / CI -->
- The hosted gates job no longer enumerates test-valued `covered_by` selectors, which recompiled the xtask test targets on the critical path; test shard 1 runs the full `check-file-policy` after its nextest build instead, so the required run loses about five minutes of wall time with the same enumeration still enforced (#11566).
