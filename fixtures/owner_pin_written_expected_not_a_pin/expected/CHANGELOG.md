# Golden Output Changes

## Pending — owner_pin_written_expected_not_a_pin (1)

Reason:
RIPR-SPEC-0108: pin owner-pin written-expected honesty case (#8795)

Command:
`cargo xtask goldens bless owner_pin_written_expected_not_a_pin --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_pin_written_expected_not_a_pin (2)

Reason:
RIPR-SPEC-0108: drop allow attribute so the match case reaches owner-pin (#8795)

Command:
`cargo xtask goldens bless owner_pin_written_expected_not_a_pin --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_pin_written_expected_not_a_pin (3)

Reason:
RIPR-SPEC-0224: ungraded oracle reads '<kind> oracle, not graded against this probe' (#8388)

Command:
`cargo xtask goldens bless owner_pin_written_expected_not_a_pin --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_pin_written_expected_not_a_pin (4)

Reason:
RIPR-SPEC-0122: explain bounded Evidence stage and state meanings for #9373 item 1; retain full-human and JSON contracts

Command:
`cargo xtask goldens bless owner_pin_written_expected_not_a_pin --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_pin_written_expected_not_a_pin (5)

Reason:
RIPR-SPEC-0122: #9373 item 1 review repair; missing discriminators include absent assertions as well as inputs

Command:
`cargo xtask goldens bless owner_pin_written_expected_not_a_pin --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_pin_written_expected_not_a_pin (6)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`
