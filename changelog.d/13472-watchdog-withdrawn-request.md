<!-- section: Internal / CI -->
- The PR staleness watchdog rechecks live request eligibility before reporting
  conflicts or requesting a retry, so withdrawn, draft, or closed requests do
  not receive stale conflict statuses or notices from the initial list (#13472).
