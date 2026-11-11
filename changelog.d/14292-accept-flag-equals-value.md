<!-- section: Changed -->
- Every command accepts the `--name=value` spelling as well as `--name value`
  (`ripr check --format=json`, `--base=origin/main`, `ripr context --at=ID`);
  tokens after a literal `--` stay as written. Switches take no value, so
  `--quiet=false` is still refused as typed rather than read as `--quiet`.
  Before, every command refused the `=` spelling as an unknown argument
  (#14292).
