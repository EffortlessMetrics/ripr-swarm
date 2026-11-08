<!-- section: Changed -->
- `ripr check --format agent` now ends with a `verify=complete|...` or
  `verify=refused|...|reason=` record projected from the producer outcome and
  full summary counts (refusing when untracked source in a language enabled
  for the run was excluded from the diff; gitignored source stays disclosed
  but does not refuse), and the generated agent loop (`ripr init --agents`)
  verifies from that same compact report instead of `--format json-compact`
  (#13516).
