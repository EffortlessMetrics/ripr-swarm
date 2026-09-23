# Golden Output Changes

## Pending — ts_repair_packet_wrong_family_oracle (1)

Reason:
RIPR-SPEC-0087: new negative fixture; a wrong-family exact-value oracle is not borrowed as the error-path repair target, so the packet stays not ready

Command:
`cargo xtask goldens bless ts_repair_packet_wrong_family_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
