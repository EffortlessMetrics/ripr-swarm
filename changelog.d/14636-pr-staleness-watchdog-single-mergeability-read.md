<!-- section: Internal / CI -->

- PR staleness watchdog: mergeability is read once per head instead of retried
  in-sweep, so a sweep no longer spends 10 s of sleep per head GitHub leaves at
  `unknown`. `mergeable_state` is unavailable for every head in the merge queue
  (63 of 102 eligible heads, which alone exceeded the 10-minute job cap and
  cancelled 136 consecutive runs). An unreadable head is classified and counted
  in the sweep summary and re-read by the next sweep; conflict notices are
  stated as unavailable for it rather than implied (#14636).
