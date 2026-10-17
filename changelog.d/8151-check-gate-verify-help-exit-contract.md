<!-- section: Docs -->
- `ripr check --help`, `ripr gate evaluate --help`, and `ripr agent verify --help`
  now state the typed 0/2/3 exit contract: findings including `exposed` and
  `weakly_exposed` still exit 0; `gate evaluate` exits 3 for `blocked` and 2
  for `config_error`; standalone `agent verify` refusals exit 3 with empty
  stdout (#5065).
