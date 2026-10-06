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
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.

Command:
`cargo xtask goldens bless match_arm_proximity_confirmation_not_credited --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
