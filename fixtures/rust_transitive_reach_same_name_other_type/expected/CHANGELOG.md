# Golden Output Changes

## Pending — rust_transitive_reach_same_name_other_type (1)

Reason:
RIPR-SPEC-0115: a unit test calling an unrelated type's same-named method no longer outranks the integration witness (#5481)

Command:
`cargo xtask goldens bless rust_transitive_reach_same_name_other_type --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_transitive_reach_same_name_other_type (2)

Reason:
RIPR-SPEC-0115: pin the named witness in full human output (#5481)

Command:
`cargo xtask goldens bless rust_transitive_reach_same_name_other_type --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_transitive_reach_same_name_other_type (3)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless rust_transitive_reach_same_name_other_type --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_transitive_reach_same_name_other_type (4)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.

Command:
`cargo xtask goldens bless rust_transitive_reach_same_name_other_type --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_transitive_reach_same_name_other_type (5)

Reason:
RIPR-SPEC-0116: the check drill-in binds --root to the resolved repository (#3948); goldens carry the <cwd>/ placeholder

Command:
`cargo xtask goldens bless rust_transitive_reach_same_name_other_type --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_transitive_reach_same_name_other_type (6)

Reason:
RIPR-SPEC-0122: a predicate's before is cut to the same span as its after (#6995)

Command:
`cargo xtask goldens bless rust_transitive_reach_same_name_other_type --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
