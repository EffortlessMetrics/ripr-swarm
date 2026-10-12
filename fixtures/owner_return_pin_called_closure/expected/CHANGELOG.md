# Golden Output Changes

## Pending — owner_return_pin_called_closure (1)

Reason:
RIPR-SPEC-0197 (#4478): shared return-oracle execution and macro admission, matched correct/wrong-library control

Command:
`cargo xtask goldens bless owner_return_pin_called_closure --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_called_closure (2)

Reason:
RIPR-SPEC-0197 (#4478): include full-human evidence for independent expected-class honesty assertions

Command:
`cargo xtask goldens bless owner_return_pin_called_closure --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — owner_return_pin_called_closure (3)

Reason:
RIPR-SPEC-0197 (#4478, #5020): test-build availability, original assertion cardinality and collection guidance; preserve matched runtime controls

Command:
`cargo xtask goldens bless owner_return_pin_called_closure --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — owner_return_pin_called_closure (4)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless owner_return_pin_called_closure --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — owner_return_pin_called_closure (5)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless owner_return_pin_called_closure --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — owner_return_pin_called_closure (6)

Reason:
RIPR-SPEC-0002: merge of origin/main (#5268 canonical_gap for Rust and sibling updates) onto the #5996 workspace-relative location owner — expected files regenerate through the merged tree

Command:
`cargo xtask goldens bless owner_return_pin_called_closure --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — owner_return_pin_called_closure (7)

Reason:
RIPR-SPEC-0116: the check drill-in binds --root to the resolved repository (#3948); goldens carry the <cwd>/ placeholder

Command:
`cargo xtask goldens bless owner_return_pin_called_closure --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
