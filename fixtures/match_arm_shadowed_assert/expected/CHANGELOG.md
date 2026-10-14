# Golden Output Changes

## Pending — match_arm_shadowed_assert (1)

Reason:
RIPR-SPEC-0229: refuse shadowed plain assertion through shared execution admission (#7788)

Command:
`cargo xtask goldens bless match_arm_shadowed_assert --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_shadowed_assert (2)

Reason:
RIPR-SPEC-0229 #7809: compact reachable-unrevealed hint names the refused assert! family; admission and full/JSON evidence unchanged

Command:
`cargo xtask goldens bless match_arm_shadowed_assert --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
