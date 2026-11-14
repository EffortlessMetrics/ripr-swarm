<!-- section: Fixed -->
- `ripr check --format agent` on a refused run (unresolvable base, invalid
  `ripr.toml`, unusable root) now prints a refusal record on stdout — the
  typed `run_status`, a one-line `stop=` cause, a `next=` command when the
  diagnostic names a runnable one, and `verify=refused` — instead of zero
  bytes an agent could read as a clean run; config file content stays on
  stderr (#14452).
