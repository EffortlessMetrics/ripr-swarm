# Golden Output Changes

## Pending — ts_coincident_state_update (1)

Reason:
RIPR-SPEC-0243: a coincident literal input on a returned state update reads weakly_exposed (#10850)

Command:
`cargo xtask goldens bless ts_coincident_state_update --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
