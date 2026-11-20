<!-- section: Fixed -->
- `ripr doctor` reports a Perl exporter that accepts the managed `ripr-facts`
  subcommand as compatible even when it has no `--version` (the canonical
  exporter exits 1 on it); the version then reads `version unknown`. Before,
  such an exporter was reported as not found. A binary that neither answers
  `--version` nor accepts `ripr-facts` is still not found (#13053).
