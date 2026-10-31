<!-- section: Fixed -->
- On Windows, `ripr` and `xtask` now place themselves in one kill-on-close Job
  Object at startup, so a force-terminated owner (`TerminateProcess`, a runner
  teardown, an editor force-kill) no longer leaks long-running descendants
  such as `cargo test`. The per-child jobs and the ordinary cleanup on exit,
  timeout and error are unchanged; the new job is best effort and a no-op on
  other platforms (#9564, #9323).
