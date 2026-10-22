# Golden Output Changes

## Pending — owner_return_pin_pointer_glob_shadow (1)

Reason:
RIPR-SPEC-0197 and RIPR-SPEC-0108: preserve the independently confirmed glob_shadow pointer-owner false-credit witness as one non-exposed return finding; matched neutral and changed-owner runtime tests both pass.

Command:
`cargo xtask goldens bless owner_return_pin_pointer_glob_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
