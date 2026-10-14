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
