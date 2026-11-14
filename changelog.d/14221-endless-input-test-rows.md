<!-- section: Fixed -->
- Added the end-to-end endless-input rows that #14137 left to manual runs:
  `receipt check --ledger`, `pr-comments summary --raw -`, and the two
  `first-pr` flags (`--check-output`, `--gap-ledger`), so a revert to an
  unbounded reader on any of them is caught (#14221).
