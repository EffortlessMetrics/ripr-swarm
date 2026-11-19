<!-- section: Internal / CI -->
- The Windows nightly builds its tests again: the Unix-only `cli_smoke`
  pilot-timeout module has its `cfg(unix)` gate back, and the fixture-producer
  step counts the four procfs tests that are pending on Windows (#15043).
