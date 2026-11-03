<!-- section: Added -->
- `ripr explain --format agent` prints the selected finding as the compact
  `check --format agent` line style with an explicit `done=` / `open=` /
  `stop=` terminal line and a `next=` `ripr context` command. `--format json`
  is refused with a pointer to `ripr context`, which stays JSON (#12104).
