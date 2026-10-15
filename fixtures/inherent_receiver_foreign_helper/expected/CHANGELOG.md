# Golden Output Changes

## Pending — inherent_receiver_foreign_helper (1)

Reason:
RIPR-SPEC-0005 and RIPR-SPEC-0108: initialize the new native foreign-helper negative; repository refusal and genuine helper positives are independently pinned. Existing expectations are unchanged.

Command:
`cargo xtask goldens bless inherent_receiver_foreign_helper --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
