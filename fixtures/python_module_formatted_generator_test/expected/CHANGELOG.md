# Golden Output Changes

## Pending — python_module_formatted_generator_test (1)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: explicit comparison removes normalized-boolean confound; executed formatted-generator test never observes changed value

Command:
`cargo xtask goldens bless python_module_formatted_generator_test --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_module_formatted_generator_test (2)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: require full human output for typed non-promotion honesty assertions; same conservative analyzer result

Command:
`cargo xtask goldens bless python_module_formatted_generator_test --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_module_formatted_generator_test (3)

Reason:
RIPR-SPEC-0028: human output regenerated after merging main's summary-terms lines

Command:
`cargo xtask goldens bless python_module_formatted_generator_test --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_module_formatted_generator_test (4)

Reason:
RIPR-SPEC-0319: main #9449 added Next guidance lines under static limitations without reblessing these fixtures (inherited red goldens); human.txt also carries this PR's RIPR-SPEC-0122 clipped-to-wrap line

Command:
`cargo xtask goldens bless python_module_formatted_generator_test --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_module_formatted_generator_test (5)

Reason:
RIPR-SPEC-0122: name the no-canonical-gap recovery instead of routing back to the gate (#9244)

Command:
`cargo xtask goldens bless python_module_formatted_generator_test --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_module_formatted_generator_test (6)

Reason:
RIPR-SPEC-0122: state only that the finding has no canonical gap, without a discriminator claim (#9244)

Command:
`cargo xtask goldens bless python_module_formatted_generator_test --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
