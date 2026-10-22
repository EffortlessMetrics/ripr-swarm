# Golden Output Changes

## Pending — owner_return_pin_pointer_glob_shadow (1)

Reason:
RIPR-SPEC-0197 and RIPR-SPEC-0108: preserve the independently confirmed glob_shadow pointer-owner false-credit witness as one non-exposed return finding; matched neutral and changed-owner runtime tests both pass.

Command:
`cargo xtask goldens bless owner_return_pin_pointer_glob_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_pointer_glob_shadow (2)

Reason:
RIPR-SPEC-0197 / #9478: update only the human repair-refusal explanation already accepted on exactmain12733. The exact-main native renderer reproduced the same4old snapshot mismatches before edit. All checkJSON, source inputs, diffs and nonpromotion oracles must remain unchanged.

Command:
`cargo xtask goldens bless owner_return_pin_pointer_glob_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
