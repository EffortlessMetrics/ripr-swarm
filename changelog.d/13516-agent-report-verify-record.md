<!-- section: Changed -->
- `ripr check --format agent` now ends with a `verify=complete|...` or
  `verify=refused|...|reason=` record projected from the producer outcome and
  full summary counts, and the generated agent loop (`ripr init --agents`)
  verifies from that same compact report instead of `--format json-compact`
  (#13516).
