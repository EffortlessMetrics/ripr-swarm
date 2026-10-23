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

## Pending — rust_returned_type_drop_if_arm (4)

Reason:
RIPR-SPEC-0122: explain bounded Evidence stage and state meanings for #9373 item 1; retain full-human and JSON contracts

Command:
`cargo xtask goldens bless rust_returned_type_drop_if_arm --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_returned_type_drop_if_arm (5)

Reason:
RIPR-SPEC-0122: #9373 item 1 review repair; missing discriminators include absent assertions as well as inputs

Command:
`cargo xtask goldens bless rust_returned_type_drop_if_arm --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_returned_type_drop_if_arm (6)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`
