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
