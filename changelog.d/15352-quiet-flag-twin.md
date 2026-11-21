<!-- section: Fixed -->
- `ripr agent repair` and `ripr reports ci-packet` accept `--quiet`, the flag twin of the `RIPR_QUIET` switch they already honored; `ci-packet` runs each step with `RIPR_QUIET=1`. The `RIPR_QUIET` help row now names every command that takes `--quiet` (#15352).
