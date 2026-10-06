# Golden Output Changes

## Pending — owner_return_pin_identity_traps (1)

Reason:
RIPR-SPEC-0197: initial golden for owner-return pins (#4478)

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (2)

Reason:
RIPR-SPEC-0122 / #4520: bounded human check output leads the exposure line with the plain word the summary uses

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (3)

Reason:
RIPR-SPEC-0197 (#4478) acceptance fixture re-rendered in main's format: every identity trap stays non-exposed; per-finding confidence tracks main's current scoring, the 0-exposed contract is unchanged

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (4)

Reason:
RIPR-SPEC-0122: check names the one-step ripr agent stub route for Rust value gaps (#5355)

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
## Pending — owner_return_pin_identity_traps (5)

Reason:
RIPR-SPEC-0224: related tests ripr examined stay listed and each names why it misses the change (#5344); verdicts unchanged

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (6)

Reason:
RIPR-SPEC-0224, #5508: observation_unconfirmed now reads 'ripr could not confirm that this assertion observes the changed behavior', an unknown rather than a claim that the assertion misses. The miss token and every decision field are unchanged.

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (7)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (8)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (9)

Reason:
RIPR-SPEC-0002: merge of origin/main (#5268 canonical_gap for Rust and sibling updates) onto the #5996 workspace-relative location owner — expected files regenerate through the merged tree

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
