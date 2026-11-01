# Golden Output Changes

## Pending — boundary_gap_equality_closed (1)

Reason:
RIPR-SPEC-0001: pin the #1429 case-3 closed variant — same canonical probe as boundary_gap moves to exposed once the exact equality assertion observes the boundary

Command:
`cargo xtask goldens bless boundary_gap_equality_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — boundary_gap_equality_closed (2)

Reason:
RIPR-SPEC-0158: ripr-main merge sync re-bless to swarm analyzer output; ripr-side behavior not carried, see merge PR residuals

Command:
`cargo xtask goldens bless boundary_gap_equality_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
