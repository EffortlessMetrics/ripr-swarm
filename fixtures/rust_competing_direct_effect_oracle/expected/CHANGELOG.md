# Golden Output Changes

## Pending — rust_competing_direct_effect_oracle (1)

Reason:
RIPR-SPEC-0094: a second production sink write masks the changed write removal; originals and removals each pass 2/2

Command:
`cargo xtask goldens bless rust_competing_direct_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
