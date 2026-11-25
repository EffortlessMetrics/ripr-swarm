<!-- section: Fixed -->
- `policy operations`, `policy history` and `policy promote` now refuse a
  missing or file `--root` with the shared `<command> root <path> is not a
  directory` message instead of exiting 0 and writing their default reports
  under the current directory (#14623).
