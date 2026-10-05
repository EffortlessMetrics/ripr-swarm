<!-- section: Fixed -->
- The `ripr check` drill-in commands (`ripr explain`, `ripr context`, the
  `ripr check --json` listing and `ripr agent stub`) now print the repository
  `check` resolved as an absolute `--root`, and a relative `--diff` or `--from`
  as an absolute path, not the relative spelling repeated as typed. Pasted from
  another directory, they analyze the same repository instead of whatever sits
  at that relative path there (#3948).
