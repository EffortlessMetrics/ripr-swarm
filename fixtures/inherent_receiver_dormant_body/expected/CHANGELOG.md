# Golden Output Changes

## Pending — inherent_receiver_dormant_body (1)

Reason:
RIPR-SPEC-0005 and RIPR-SPEC-0108: initialize the native dormant-body refusal; declaration syntax cannot supply executed owner credit. Existing goldens and runtime labels are unchanged.

Command:
`cargo xtask goldens bless inherent_receiver_dormant_body --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — inherent_receiver_dormant_body (2)

Reason:
RIPR-SPEC-0122: integrate main #8139 root-relative Hidden locations; only the fixture-root prefix changes to ./src, with source lines, classes and all JSON observations unchanged.

Command:
`cargo xtask goldens bless inherent_receiver_dormant_body --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — inherent_receiver_dormant_body (3)

Reason:
RIPR-SPEC-0002 / #7756 / #8004 integration: adopt the labelled-field and list-item terminal-period rule landed in fdcb75cf3f9acc928967d8ab4764d0caf8dbb31b (#8096). Only the independently audited terminal periods are removed; classifications, counts, evidence, recommendations, JSON, native truth labels and corpus claims are unchanged.

Verification:
`cargo xtask goldens check` after an audited line-only adjustment.

Updated:
- `expected/human.txt` (4 terminal periods)
