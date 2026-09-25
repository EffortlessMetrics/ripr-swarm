# Golden Output Changes

## Pending — ts_repair_packet_boundary_unreachable (1)

Reason:
RIPR-SPEC-0087/#4105: new fixture pins that a complete TS repair packet must fail closed and emit a boundary placeholder shape when the observed call input provably cannot reach the named discriminator boundary

Command:
`cargo xtask goldens bless ts_repair_packet_boundary_unreachable --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
