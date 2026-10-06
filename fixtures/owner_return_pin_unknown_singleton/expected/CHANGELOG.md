# Golden Output Changes

## Pending — owner_return_pin_unknown_singleton (1)

Reason:
RIPR-SPEC-0197 (#4478, #5020): test-build availability, original assertion cardinality and collection guidance; preserve matched runtime controls

Command:
`cargo xtask goldens bless owner_return_pin_unknown_singleton --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — owner_return_pin_unknown_singleton (2)

Reason:
RIPR-SPEC-0122: check names the one-step ripr agent stub route for Rust value gaps (#5355)

Command:
`cargo xtask goldens bless owner_return_pin_unknown_singleton --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
## Pending — owner_return_pin_unknown_singleton (3)

Reason:
RIPR-SPEC-0224: related tests ripr examined stay listed and each names why it misses the change (#5344); verdicts unchanged

Command:
`cargo xtask goldens bless owner_return_pin_unknown_singleton --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — owner_return_pin_unknown_singleton (4)

Reason:
RIPR-SPEC-0224: a matched related test keeps its oracle kind and strength in full output and adds why it still misses; verdicts unchanged

Command:
`cargo xtask goldens bless owner_return_pin_unknown_singleton --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — owner_return_pin_unknown_singleton (5)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless owner_return_pin_unknown_singleton --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — owner_return_pin_unknown_singleton (6)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless owner_return_pin_unknown_singleton --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — owner_return_pin_unknown_singleton (7)

Reason:
RIPR-SPEC-0002: merge of origin/main (#5268 canonical_gap for Rust and sibling updates) onto the #5996 workspace-relative location owner — expected files regenerate through the merged tree

Command:
`cargo xtask goldens bless owner_return_pin_unknown_singleton --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — owner_return_pin_unknown_singleton (8)

Reason:
RIPR-SPEC-0197: re-apply #5359 refusal disclosure (Why unrevealed / Not credited / owner-calling next step) on top of main's workspace-relative locations and canonical gap lines after merging main

Command:
`cargo xtask goldens bless owner_return_pin_unknown_singleton --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — owner_return_pin_unknown_singleton (9)

Reason:
RIPR-SPEC-0240: a non-limit assertion refusal no longer offers a static-limit reading (#6903)

Command:
`cargo xtask goldens bless owner_return_pin_unknown_singleton --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
