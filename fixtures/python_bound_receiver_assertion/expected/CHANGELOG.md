# Golden Output Changes

## Pending — python_bound_receiver_assertion (1)

Reason:
RIPR-SPEC-0028: new control fixture for #8247; an asserted call on the owner-bound receiver keeps exposed credit beside a foreign receiver

Command:
`cargo xtask goldens bless python_bound_receiver_assertion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_bound_receiver_assertion (2)

Reason:
RIPR-SPEC-0028: add required human-full projection for the RIPR-SPEC-0108 honesty case

Command:
`cargo xtask goldens bless python_bound_receiver_assertion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_bound_receiver_assertion (3)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless python_bound_receiver_assertion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
