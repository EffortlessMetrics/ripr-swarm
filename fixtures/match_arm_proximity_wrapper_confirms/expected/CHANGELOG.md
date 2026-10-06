# Golden Output Changes

## Pending — match_arm_proximity_wrapper_confirms (1)

Reason:
RIPR-SPEC-0094: a same-file test that calls a wrapper of the owner still confirms a match arm beside an owner-calling test (#6297 control)

Command:
`cargo xtask goldens bless match_arm_proximity_wrapper_confirms --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_proximity_wrapper_confirms (2)

Reason:
RIPR-SPEC-0094: fixture patch trailing blank context line removed (#6297); input identity only

Command:
`cargo xtask goldens bless match_arm_proximity_wrapper_confirms --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_proximity_wrapper_confirms (3)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.

Command:
`cargo xtask goldens bless match_arm_proximity_wrapper_confirms --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
