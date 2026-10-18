<!-- section: Fixed -->
- `ripr repair` and `ripr agent repair` now accept the `probe:...` finding ID
  and `file:line` location `ripr check` prints when they uniquely name an
  eligible seam, `ripr explain`/`context` accept a seam ID that uniquely binds
  a finding in the selected diff, ambiguous or stale cross-namespace IDs refuse
  with the equivalent correct command and start no attempt, and static-limited
  findings no longer print a runnable `ripr repair` line beside their
  limitation guidance (#8166).
