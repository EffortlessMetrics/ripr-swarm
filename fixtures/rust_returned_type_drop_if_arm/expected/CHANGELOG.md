# Golden Output Changes

## Pending — rust_returned_type_drop_if_arm (1)

Reason:
RIPR-SPEC-0197: a non-plain Drop record in an if-branch tail that feeds the function result still demotes to weakly_exposed

Command:
`cargo xtask goldens bless rust_returned_type_drop_if_arm --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_returned_type_drop_if_arm (2)

Reason:
RIPR-SPEC-0197: a non-plain Drop record in an if-branch tail that feeds the function result still demotes to weakly_exposed

Command:
`cargo xtask goldens bless rust_returned_type_drop_if_arm --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_returned_type_drop_if_arm (3)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless rust_returned_type_drop_if_arm --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
