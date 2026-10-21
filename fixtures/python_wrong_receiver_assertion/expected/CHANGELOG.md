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

## Pending — python_wrong_receiver_assertion (3)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless python_wrong_receiver_assertion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_wrong_receiver_assertion (4)

Reason:
RIPR-SPEC-0224 / #5491: Python related-test rows now carry truthful typed miss and why (explanation only; class, stages, order unchanged)

Command:
`cargo xtask goldens bless python_wrong_receiver_assertion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
