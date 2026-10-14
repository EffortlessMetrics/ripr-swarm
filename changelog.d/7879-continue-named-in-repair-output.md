<!-- section: Fixed -->
- Ordinary before-phase and `ripr status` next-step lines now name
  `ripr continue --root … --attempt …`, matching `ripr repair --help`. The
  persisted attempt `next_command` and `--json` continuation keep the
  advanced `ripr agent repair --phase after` spelling; trust-bound after
  routes still print the authorization flags `ripr continue` cannot supply
  (#7879).
