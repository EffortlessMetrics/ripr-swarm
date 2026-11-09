<!-- section: Internal / CI -->
- The PR staleness watchdog rechecks live request eligibility during mergeability
  polling and immediately before writing a conflict report, cleared status, or
  re-apply notice, so requests withdrawn, drafted, closed, or moved during the
  sweep do not receive stale writes (#13472).
