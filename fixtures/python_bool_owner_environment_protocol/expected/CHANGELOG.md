# Golden Output Changes

## Pending — python_bool_owner_environment_protocol (1)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: preserve conservative classification and withhold bare-bool exact pins; native wrong-boundary control still passes

Command:
`cargo xtask goldens bless python_bool_owner_environment_protocol --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
