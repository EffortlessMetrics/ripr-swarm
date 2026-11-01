# Golden Output Changes

## Pending — error_return_unresolved_guard (1)

Reason:
RIPR-SPEC-0158 #1579: bless precise guard-unresolved limitation output (infection_unknown, typed limitation, no impossible prescription)

Command:
`cargo xtask goldens bless error_return_unresolved_guard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — error_return_unresolved_guard (2)

Reason:
RIPR-SPEC-0158: ripr-main merge sync re-bless to swarm analyzer output; ripr-side behavior not carried, see merge PR residuals

Command:
`cargo xtask goldens bless error_return_unresolved_guard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
