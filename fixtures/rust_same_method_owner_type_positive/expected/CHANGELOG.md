# Golden Output Changes

## Pending — rust_same_method_owner_type_positive (1)

Reason:
RIPR-SPEC-0108: pin #4760 owner-type receiver identity keeps exposed

Command:
`cargo xtask goldens bless rust_same_method_owner_type_positive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_same_method_owner_type_positive (2)

Reason:
RIPR-SPEC-0108: include human-full for #4760 owner-type positive control

Command:
`cargo xtask goldens bless rust_same_method_owner_type_positive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_same_method_owner_type_positive (3)

Reason:
merge origin/main: RIPR-SPEC-0122 renderer (#4320 windows, #4322 canonical summary tokens, #4324 stage Evidence line) re-renders these #4760 fixtures; diff.patch declared hunk span corrected per #4439 so input_identity updates; classifications and counts unchanged

Command:
`cargo xtask goldens bless rust_same_method_owner_type_positive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_same_method_owner_type_positive (4)

Reason:
RIPR-SPEC-0224: related tests ripr examined stay listed and each names why it misses the change (#5344); verdicts unchanged

Command:
`cargo xtask goldens bless rust_same_method_owner_type_positive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_same_method_owner_type_positive (5)

Reason:
RIPR-SPEC-0224: a matched related test keeps its oracle kind and strength in full output and adds why it still misses; verdicts unchanged

Command:
`cargo xtask goldens bless rust_same_method_owner_type_positive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_same_method_owner_type_positive (6)

Reason:
RIPR-SPEC-0224, #5508: observation_unconfirmed now reads 'ripr could not confirm that this assertion observes the changed behavior'; human-full re-blessed after rebase onto #5424. No verdict change.

Command:
`cargo xtask goldens bless rust_same_method_owner_type_positive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_same_method_owner_type_positive (7)

Reason:
RIPR-SPEC-0224, #5508: an observation_unconfirmed row is labelled 'unconfirmed:' instead of 'misses:' in human-full. No verdict change.

Command:
`cargo xtask goldens bless rust_same_method_owner_type_positive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_same_method_owner_type_positive (8)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless rust_same_method_owner_type_positive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_same_method_owner_type_positive (9)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless rust_same_method_owner_type_positive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_same_method_owner_type_positive (10)

Reason:
RIPR-SPEC-0002: merge of origin/main (#5268 canonical_gap for Rust and sibling updates) onto the #5996 workspace-relative location owner — expected files regenerate through the merged tree

Command:
`cargo xtask goldens bless rust_same_method_owner_type_positive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
