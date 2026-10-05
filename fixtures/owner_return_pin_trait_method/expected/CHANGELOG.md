# Golden Output Changes

## Pending — owner_return_pin_trait_method (1)

Reason:
RIPR-SPEC-0197: initial golden for owner-return pins (#4478)

Command:
`cargo xtask goldens bless owner_return_pin_trait_method --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_trait_method (2)

Reason:
RIPR-SPEC-0197 (#4478) acceptance fixture re-rendered in main's #4520 exposure-word format: the trait default method stays pinned exposed through the byte-slice receiver

Command:
`cargo xtask goldens bless owner_return_pin_trait_method --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_trait_method (3)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless owner_return_pin_trait_method --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_trait_method (4)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.

Command:
`cargo xtask goldens bless owner_return_pin_trait_method --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
