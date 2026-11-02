# Golden Output Changes

## Pending — typescript_snapshot_effect_conditional_receiver (1)

Reason:
RIPR-SPEC-0258 rule 3: owner name bound elsewhere and non-constructed receiver earn no snapshot credit (#12465)

Command:
`cargo xtask goldens bless typescript_snapshot_effect_conditional_receiver --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_snapshot_effect_conditional_receiver (2)

Reason:
RIPR-SPEC-0258 rule 3: retain human-full golden for the honesty corpus (#12465)

Command:
`cargo xtask goldens bless typescript_snapshot_effect_conditional_receiver --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
