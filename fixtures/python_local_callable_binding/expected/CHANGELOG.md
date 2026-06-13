# Golden Output Changes

## Pending

Reason:
document the local-callable-instance limitation (#1172): conservative weakly_exposed pinned; target exposed when relation+oracle-extraction traces local bindings

Command:
`cargo xtask goldens bless python_local_callable_binding --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
re-bless: #1204 added related_tests_total to JSON output after #1205 blessed this golden; restore main goldens

Command:
`cargo xtask goldens bless python_local_callable_binding --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
