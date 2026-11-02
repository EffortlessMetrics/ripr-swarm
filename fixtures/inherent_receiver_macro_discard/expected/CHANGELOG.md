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

## Pending — inherent_receiver_macro_discard (2)

Reason:
RIPR-SPEC-0122: integrate main #8139 root-relative Hidden locations; only the fixture-root prefix changes to ./src, with source lines, classes and all JSON observations unchanged.

Command:
`cargo xtask goldens bless inherent_receiver_macro_discard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — inherent_receiver_macro_discard (3)

Reason:
RIPR-SPEC-0002 / #7756 / #8004 integration: adopt the labelled-field and list-item terminal-period rule landed in fdcb75cf3f9acc928967d8ab4764d0caf8dbb31b (#8096). Only the independently audited terminal periods are removed; classifications, counts, evidence, recommendations, JSON, native truth labels and corpus claims are unchanged.

Verification:
`cargo xtask goldens check` after an audited line-only adjustment.

Updated:
- `expected/human-full.txt` (4 terminal periods)
- `expected/human.txt` (4 terminal periods)

## Pending — inherent_receiver_macro_discard (4)

Reason:
RIPR-SPEC-0122: refresh to main's plain-language human output wording; no verdict or class change (#8004 merge)

Command:
`cargo xtask goldens bless inherent_receiver_macro_discard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — inherent_receiver_macro_discard (5)

Reason:
RIPR-SPEC-0005: #7415 decoy demotion to weak_token_substring plus #9342 (0bfaa83289 / #9169): the ignored_operand finding weakly_exposed -> reachable_unrevealed (a demoted decoy never credits an oracle); less credit, no new exposed, plus next-step wording from #10886/#10887

Command:
`cargo xtask goldens bless inherent_receiver_macro_discard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — inherent_receiver_macro_discard (6)

Reason:
RIPR-SPEC-0122: incomplete evidence stage tokens render as plain words in the human repair advisory (#11025)

Command:
`cargo xtask goldens bless inherent_receiver_macro_discard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — inherent_receiver_macro_discard (7)

Reason:
RIPR-SPEC-0122: #9373 hidden rows name the unknown stage and gloss listed classes

Command:
`cargo xtask goldens bless inherent_receiver_macro_discard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — inherent_receiver_macro_discard (8)

Reason:
RIPR-SPEC-0118 / #10692: plain wording for seam and discriminator in default check and repair-start text; verdicts, identity and JSON fields unchanged

Command:
`cargo xtask goldens bless inherent_receiver_macro_discard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — inherent_receiver_macro_discard (9)

Reason:
RIPR-SPEC-0122: explain missing input or assertion honestly

Command:
`cargo xtask goldens bless inherent_receiver_macro_discard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
