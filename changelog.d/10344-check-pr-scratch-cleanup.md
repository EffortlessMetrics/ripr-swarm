<!-- section: Internal / CI -->
- The `check_pr_failure_report_replaces_stale_success_report` xtask test retries
  removing its own scratch directory for up to five seconds after a Windows
  sharing or lock violation, rather than failing after its oracle has
  passed (#10344).
