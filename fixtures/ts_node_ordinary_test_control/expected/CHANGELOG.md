# Golden Output Changes

## Pending — ts_node_ordinary_test_control (1)

Reason:
RIPR-SPEC-0108: pin Node expectFailure registrations withholding ordinary oracle credit (#5436)

Command:
`cargo xtask goldens bless ts_node_ordinary_test_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_node_ordinary_test_control (2)

Reason:
RIPR-SPEC-0108: add human-full golden for the honesty-corpus projection (#5436)

Command:
`cargo xtask goldens bless ts_node_ordinary_test_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — ts_node_ordinary_test_control (3)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless ts_node_ordinary_test_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — ts_node_ordinary_test_control (4)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.

Command:
`cargo xtask goldens bless ts_node_ordinary_test_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
