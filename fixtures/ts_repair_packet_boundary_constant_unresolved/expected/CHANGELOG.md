# Golden Output Changes

## Pending — ts_repair_packet_boundary_constant_unresolved (1)

Reason:
RIPR-SPEC-0087/#4215: new fixture pins that a TS repair packet fails closed with a boundary placeholder when the discriminator's boundary is an unresolved named constant and no observed argument names it

Command:
`cargo xtask goldens bless ts_repair_packet_boundary_constant_unresolved --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_repair_packet_boundary_constant_unresolved (2)

Reason:
RIPR-SPEC-0087/#4215: pin the human-full target shape placeholder for the unresolved named-constant boundary

Command:
`cargo xtask goldens bless ts_repair_packet_boundary_constant_unresolved --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — ts_repair_packet_boundary_constant_unresolved (3)

Reason:
RIPR-SPEC-0122 (#4216): TS/JS preview safe next action is terminal for a closed repair packet (quotes the validator's why_not_actionable) and says no repair for an exposed finding

Command:
`cargo xtask goldens bless ts_repair_packet_boundary_constant_unresolved --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
