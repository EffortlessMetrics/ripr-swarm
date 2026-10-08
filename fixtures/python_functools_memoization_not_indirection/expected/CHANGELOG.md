# Golden Output Changes

## Pending — python_functools_memoization_not_indirection (1)

Reason:
RIPR-SPEC-0028: functools lru_cache is not decorator_indirection; credited exact twin and smoke-gap twin

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (2)

Reason:
RIPR-SPEC-0028: align fixture hunk with source line numbers

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (3)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (4)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.
RIPR-SPEC-0140: adopt intended config_identity hash from #6777 missed by its re-bless sweep (#6818)

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (5)

Reason:
RIPR-SPEC-0002: merge of origin/main onto the #5996 workspace-relative location owner — the expected files regenerate with main's triage/wording updates and the shared root-relative location form (issue #5996)

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (6)

Reason:
RIPR-SPEC-0224: record test_assertion_admission evidence for assertion-free related tests (#5571)

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (7)

Reason:
RIPR-SPEC-0116: the check drill-in binds --root to the resolved repository (#3948); goldens carry the <cwd>/ placeholder

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
