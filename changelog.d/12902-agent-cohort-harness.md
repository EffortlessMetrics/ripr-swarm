<!-- section: Added -->
- Opt-in agent cohort collection and reference-qualification tools now use the
  repaired scoring/accounting core; both reporting entrypoints preserve failed
  attempts and unknown costs. Collection stops owned tools after a timeout before
  grading, confirms POSIX group quiescence from complete supported states, and
  withholds grading and success when cleanup cannot be established, including
  unrecognized states and zombie leaders with possible live threads. Reference
  collection also requires a successful JSON terminal result before accepting
  tests. Original datasets remain explicitly unrequalified historical observations
  (#12902, #12912).
