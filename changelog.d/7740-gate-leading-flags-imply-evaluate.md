<!-- section: Fixed -->
- `ripr gate --pr-guidance <path> ...` now implies `evaluate` when the first
  argument is a flag, matching bare `ripr gate` (#2013). A leading
  `--pr-guidance` is no longer reported as an unknown subcommand (#7740).
