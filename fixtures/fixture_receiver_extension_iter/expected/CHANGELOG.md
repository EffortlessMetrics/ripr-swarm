# Golden Output Changes

## Pending — fixture_receiver_extension_iter (1)

Reason:
RIPR-SPEC-0240 #9766: an extension iterator discards the literal receiver data before the changed projection; preserve the independently confirmed no-credit control

Command:
`cargo xtask goldens bless fixture_receiver_extension_iter --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
