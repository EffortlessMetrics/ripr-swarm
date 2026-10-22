# Golden Output Changes

## Pending — rust_same_test_disconnected_effect_oracle (1)

Reason:
RIPR-SPEC-0094: actual dispatch cannot lend kind-only credit to parser-before/parser-after or fresh-handle equality; 4/4 originals and compiling removals survive

Command:
`cargo xtask goldens bless rust_same_test_disconnected_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt` (same validated run)

## Pending — rust_same_test_disconnected_effect_oracle (2)

Reason:
RIPR-SPEC-0094: refresh human renderer wording after merging main; formatting-only

Command:
`cargo xtask goldens bless rust_same_test_disconnected_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_same_test_disconnected_effect_oracle (3)

Reason:
RIPR-SPEC-0094: merged main; reach summary no longer lists parser-only decoy tests and human output carries main's repair-not-ready line; verdicts unchanged

Command:
`cargo xtask goldens bless rust_same_test_disconnected_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
