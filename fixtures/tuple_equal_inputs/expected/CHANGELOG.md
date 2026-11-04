# Golden Output Changes

## Pending — tuple_equal_inputs (1)

Reason:
RIPR-SPEC-0229: zero tuple-method inputs produce the same result on both sides of the diff

Command:
`cargo xtask goldens bless tuple_equal_inputs --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — tuple_equal_inputs (2)

Reason:
RIPR-SPEC-0229: retain full human projection for the equal-input tuple witness

Command:
`cargo xtask goldens bless tuple_equal_inputs --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — tuple_equal_inputs (3)

Reason:
RIPR-SPEC-0229: name the established equal-input witness in the separating-input guidance

Command:
`cargo xtask goldens bless tuple_equal_inputs --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — tuple_equal_inputs (4)

Reason:
RIPR-SPEC-0229: request a field assertion without assuming whole-value equality observes the tuple field

Command:
`cargo xtask goldens bless tuple_equal_inputs --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
