# Golden Output Changes

## Pending — rust_computed_return_tail_reached (1)

Reason:
RIPR-SPEC-0443: a reached computed tail is observed by its exact pin; the other branch's pin is not (#13037)

Command:
`cargo xtask goldens bless rust_computed_return_tail_reached --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_computed_return_tail_reached (2)

Reason:
RIPR-SPEC-0443: add the human-full projection; identity and constant-scope gates (#13037)

Command:
`cargo xtask goldens bless rust_computed_return_tail_reached --reason "..."`

Updated:
- `expected/human-full.txt`

## Pending — rust_computed_return_tail_reached (3)

Reason:
RIPR-SPEC-0122: #15313 Summary names shared candidate lines and says No test gap when State reads no test gap found

Command:
`cargo xtask goldens bless rust_computed_return_tail_reached --reason "..."`

Updated:
- `expected/human.txt`
