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
RIPR-SPEC-0319 next-step lines: fixture goldens blessed without them (#10235 after #9449 combined-tree drift)

Command:
`cargo xtask goldens bless python_module_formatted_generator_test --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
