# Golden Output Changes

## Pending — predicate_boundary_oracle_refused (1)

Reason:
RIPR-SPEC-0197/#5027 and SPEC0186: a refused boundary equality cannot borrow an admitted far oracle; retain far strength/observation but require weak discrimination. Exact compiled mutant controls pass both tests.

Command:
`cargo xtask goldens bless predicate_boundary_oracle_refused --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — predicate_boundary_oracle_refused (2)

Reason:
RIPR-SPEC-0122: check names the one-step ripr agent stub route for Rust value gaps (#5355)

Command:
`cargo xtask goldens bless predicate_boundary_oracle_refused --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
## Pending — predicate_boundary_oracle_refused (3)

Reason:
RIPR-SPEC-0224: related tests ripr examined stay listed and each names why it misses the change (#5344); verdicts unchanged

Command:
`cargo xtask goldens bless predicate_boundary_oracle_refused --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — predicate_boundary_oracle_refused (4)

Reason:
RIPR-SPEC-0224: a matched related test keeps its oracle kind and strength in full output and adds why it still misses; verdicts unchanged

Command:
`cargo xtask goldens bless predicate_boundary_oracle_refused --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — predicate_boundary_oracle_refused (5)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless predicate_boundary_oracle_refused --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — predicate_boundary_oracle_refused (6)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.

Command:
`cargo xtask goldens bless predicate_boundary_oracle_refused --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
