# Golden Output Changes

## Pending — ts_repair_packet_boundary_constant_first (1)

Reason:
RIPR-SPEC-0087/#4215 review: new fixture pins that a constant written first (DISCOUNT_THRESHOLD <= amount) also fails the TS repair packet closed

Command:
`cargo xtask goldens bless ts_repair_packet_boundary_constant_first --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — ts_repair_packet_boundary_constant_first (2)

Reason:
RIPR-SPEC-0122 (#4216): TS/JS preview safe next action is terminal for a closed repair packet (quotes the validator's why_not_actionable) and says no repair for an exposed finding

Command:
`cargo xtask goldens bless ts_repair_packet_boundary_constant_first --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — ts_repair_packet_boundary_constant_first (3)

Reason:
RIPR-SPEC-0122 (#4216 review): closed-packet TS/JS safe action bounds the quoted reason, drops the causal 'so', and asks unknown-class findings for a manual check

Command:
`cargo xtask goldens bless ts_repair_packet_boundary_constant_first --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
