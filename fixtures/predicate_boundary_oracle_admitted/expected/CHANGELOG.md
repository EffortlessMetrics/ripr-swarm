# Golden Output Changes

## Pending — predicate_boundary_oracle_admitted (1)

Reason:
RIPR-SPEC-0197/#5027 and SPEC0186: matched direct boundary positive remains exposed and fails the compiled boundary mutant; separate far assertion retains independent meaning.

Command:
`cargo xtask goldens bless predicate_boundary_oracle_admitted --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — predicate_boundary_oracle_admitted (2)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless predicate_boundary_oracle_admitted --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — predicate_boundary_oracle_admitted (3)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.

Command:
`cargo xtask goldens bless predicate_boundary_oracle_admitted --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
