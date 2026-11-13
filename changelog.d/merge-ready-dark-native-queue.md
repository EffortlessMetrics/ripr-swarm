<!-- section: Internal / CI -->
- `.github/workflows/pr-staleness-watchdog.yml` and
  `merge-ready-conflict-notice.yml` direct authors to enqueue through the native
  queue after green `merge-ready` qualification instead of implying that
  re-applying the label enqueues the PR (#10136).
