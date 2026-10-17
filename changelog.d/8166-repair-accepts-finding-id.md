<!-- section: Fixed -->
- `ripr repair <probe-id>` and `ripr agent repair --seam-id <probe-id>` now
  accept the `probe:` / `repo-probe:` finding ID `ripr check` prints, instead
  of the blanket "finding ID, not a repair subject" rejection. The ID
  resolves through the current analysis to the one seam the check
  drill-down names, and the repair-eligibility gate still applies; an
  unknown or stale ID refuses with the recovery route and starts no
  attempt. `ripr explain` / `ripr context` misses on a 16-hex seam ID now
  name `ripr repair <id>` as the equivalent command instead of dead-ending,
  and a finding under a named static limitation no longer sits under a
  runnable `ripr repair` line (#8166).
