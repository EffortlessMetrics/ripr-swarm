# Golden Output Changes

## Pending — match_arm_proximity_confirmation_not_credited (1)

Reason:
RIPR-SPEC-0094: a same-file test that names a match arm's variant, and calls nothing reaching the owner, cannot confirm the arm while another related test reaches the owner (#6297)

Command:
`cargo xtask goldens bless match_arm_proximity_confirmation_not_credited --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_proximity_confirmation_not_credited (2)

Reason:
RIPR-SPEC-0094: fixture patch trailing blank context line removed (#6297); input identity only

Command:
`cargo xtask goldens bless match_arm_proximity_confirmation_not_credited --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_proximity_confirmation_not_credited (3)

Reason:
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless match_arm_proximity_confirmation_not_credited --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
