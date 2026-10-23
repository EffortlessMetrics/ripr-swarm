# Golden Output Changes

## Pending — rust_sibling_effect_oracle (1)

Reason:
RIPR-SPEC-0094 #7794: parser-only equalities cannot confirm an aggregation effect by oracle kind

Command:
`cargo xtask goldens bless rust_sibling_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_sibling_effect_oracle (2)

Reason:
RIPR-SPEC-0094 #7794: parser-only equalities remain unconfirmed for aggregation effects

Command:
`cargo xtask goldens bless rust_sibling_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_sibling_effect_oracle (3)

Reason:
RIPR-SPEC-0094: refresh human renderer wording after merging main; formatting-only

Command:
`cargo xtask goldens bless rust_sibling_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_sibling_effect_oracle (4)

Reason:
RIPR-SPEC-0094: merged main; reach summary no longer lists parser-only decoy tests and human output carries main's repair-not-ready line; verdicts unchanged

Command:
`cargo xtask goldens bless rust_sibling_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_sibling_effect_oracle (5)

Reason:
RIPR-SPEC-0122: route-not-ready refusal names its missing evidence (#9604 batch C)

Command:
`cargo xtask goldens bless rust_sibling_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_sibling_effect_oracle (6)

Reason:
RIPR-SPEC-0197: effect assertions in the retained module need execution admission; json! at line 101 mentions format, so the line-374 assert_eq! is refused and the finding reads reachable_unrevealed (#8927)

Command:
`cargo xtask goldens bless rust_sibling_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
