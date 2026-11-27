<!-- section: Fixed -->
- `ripr check` and bare `ripr repair` under a `--root` below the Git top-level no longer print a repair
  start that the before phase then refuses; they withhold it and name the exact Git top-level to rerun
  from, and the shared repair-start gate refuses a subdirectory root the same way (#15488).
