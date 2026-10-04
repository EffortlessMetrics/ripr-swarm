# Golden Output Changes

## Pending — match_arm_expected_side_trap (1)

Reason:
RIPR-SPEC-0093: new fixture (#5432) - the only owner call selects Kind::Alpha, so a Kind::Beta token on the expected side no longer confirms the changed Kind::Beta arm

Command:
`cargo xtask goldens bless match_arm_expected_side_trap --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_expected_side_trap (2)

Reason:
RIPR-SPEC-0229: selection outranks tokens; the expected-side Kind::Beta token no longer confirms the unselected arm

Command:
`cargo xtask goldens bless match_arm_expected_side_trap --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_expected_side_trap (3)

Reason:
RIPR-SPEC-0229 merged with RIPR-SPEC-0224 (#5424): the named unselected arm and weak infection now carry into the examined-test miss reason

Command:
`cargo xtask goldens bless match_arm_expected_side_trap --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_expected_side_trap (4)

Reason:
RIPR-SPEC-0229 with RIPR-SPEC-0224: an unselected arm is a missing input (no test input selects the arm), not a missing exact assertion

Command:
`cargo xtask goldens bless match_arm_expected_side_trap --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
