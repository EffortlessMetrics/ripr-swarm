# Golden Output Changes

## Pending — match_arm_expected_side_trap (1)

Reason:
RIPR-SPEC-0093: new fixture (#5432) - the only owner call selects Kind::Alpha, so a Kind::Beta token on the expected side no longer confirms the changed Kind::Beta arm

Command:
`cargo xtask goldens bless match_arm_expected_side_trap --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
