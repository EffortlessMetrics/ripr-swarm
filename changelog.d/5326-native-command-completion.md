<!-- section: Internal / CI -->
- Timed xtask commands use Linux pidfd exit notification where available,
  preserve process-owner cleanup, and cap fallback polling sleeps at 5 ms;
  duration remains elapsed observation time including timeout cleanup (#5326).
