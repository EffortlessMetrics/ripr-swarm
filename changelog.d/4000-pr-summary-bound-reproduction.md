<!-- section: Fixed -->
- `ripr pr-summary` lists its own reproduction commands (`ripr check`,
  `ripr first-pr`) with the absolute repository it read, like the repair and
  verify commands it carries from `start-here.json`. They used to omit the
  root or print `--root .`, so pasting the list from another directory mixed
  two repositories in one sequence (#4000).
