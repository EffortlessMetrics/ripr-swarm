<!-- section: Security -->
- The Windows zip installer (the `ripr-check` action and the generated
  `ripr init --ci github` workflow) now matches the root `ripr.exe` entry as a
  literal string and refuses listings with case-insensitive or
  trailing-dot/space duplicate names (`RIPR.EXE`, `ripr.exe.`) before
  extraction. (#6245)
