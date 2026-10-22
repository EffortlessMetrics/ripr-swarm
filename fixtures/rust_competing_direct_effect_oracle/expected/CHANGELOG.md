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

## Pending — rust_competing_direct_effect_oracle (2)

Reason:
RIPR-SPEC-0094: refresh human renderer wording after merging main; formatting-only

Command:
`cargo xtask goldens bless rust_competing_direct_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_competing_direct_effect_oracle (3)

Reason:
RIPR-SPEC-0094: merged main; reach summary no longer lists parser-only decoy tests and human output carries main's repair-not-ready line; verdicts unchanged

Command:
`cargo xtask goldens bless rust_competing_direct_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_competing_direct_effect_oracle (4)

Reason:
RIPR-SPEC-0122: route-not-ready refusal names its missing evidence (#9604 batch C)

Command:
`cargo xtask goldens bless rust_competing_direct_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
