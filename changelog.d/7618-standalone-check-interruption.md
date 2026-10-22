<!-- section: Changed -->
- Standalone Unix `ripr check` cooperatively settles managed exporters on first
  SIGINT for ordinary regular-file/live-Git checks, exits 130 with repeat guidance,
  and preserves native interruption for blocking input, separate sinks and stdout
  publication. Library `cli::run` retains caller signal policy; the explicit
  `run_with_interrupt_handling` entry owns process-lifetime signals. The unendorsed
  `CommandError` shape adds `Interrupted(String)`: exhaustive matches must add that
  arm or use `message()`/`exit_code()`. This migration belongs to the upcoming
  0.11.0 minor, not a 0.11.x patch (#7618).
