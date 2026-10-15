# Golden Output Changes

## Pending — inherent_receiver_macro_discard (1)

Reason:
RIPR-SPEC-0005 and RIPR-SPEC-0108: initialize the new native discarded-macro control; preserve the retained low owner relation and no promotion. Existing expectations are unchanged.

Command:
`cargo xtask goldens bless inherent_receiver_macro_discard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
