# Golden Output Changes

## Pending — python_wrong_receiver_assertion (1)

Reason:
RIPR-SPEC-0028: new leaf fixture for #8247; an asserted method call on a receiver not bound to the owner class is not owner identity

Command:
`cargo xtask goldens bless python_wrong_receiver_assertion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_wrong_receiver_assertion (2)

Reason:
RIPR-SPEC-0028: add required human-full projection for the RIPR-SPEC-0108 honesty case

Command:
`cargo xtask goldens bless python_wrong_receiver_assertion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
