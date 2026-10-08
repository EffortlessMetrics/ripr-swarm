# Golden Output Changes

## Pending — error_path_operand_error_lexeme (1)

Reason:
RIPR-SPEC-0108: operand-position error lexeme cannot confirm ErrorPath (#5255)

Command:
`cargo xtask goldens bless error_path_operand_error_lexeme --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_operand_error_lexeme (2)

Reason:
RIPR-SPEC-0108: combined-tree refresh of operand-lexeme twin after main named related-test miss reasons (#5424); classification stays weakly_exposed / observation_unverified

Command:
`cargo xtask goldens bless error_path_operand_error_lexeme --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_operand_error_lexeme (3)

Reason:
RIPR-SPEC-0108: re-bless operand-lexeme twin to the #5578 unconfirmed-observation wording inherited from main; classification unchanged (weakly_exposed / observation_unverified), formatting-only wording drift
RIPR-SPEC-0224, #5508: align this fixture with the b2f2a2154 observation_unconfirmed sentence; blessed pre-reword, pins the retired miss sentence. formatting_only 1-line flip per surface; no verdict change.

Command:
`cargo xtask goldens bless error_path_operand_error_lexeme --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_operand_error_lexeme (4)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless error_path_operand_error_lexeme --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_operand_error_lexeme (5)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless error_path_operand_error_lexeme --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_operand_error_lexeme (6)

Reason:
RIPR-SPEC-0002: merge of origin/main (#5268 canonical_gap for Rust and sibling updates) onto the #5996 workspace-relative location owner — expected files regenerate through the merged tree

Command:
`cargo xtask goldens bless error_path_operand_error_lexeme --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_operand_error_lexeme (7)

Reason:
RIPR-SPEC-0116: the check drill-in binds --root to the resolved repository (#3948); goldens carry the <cwd>/ placeholder

Command:
`cargo xtask goldens bless error_path_operand_error_lexeme --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_operand_error_lexeme (8)

Reason:
RIPR-SPEC-0122 #5471: stub route printed only when the resolver yields a stub, with --kind; refusal or nothing otherwise (merge re-bless)

Command:
`cargo xtask goldens bless error_path_operand_error_lexeme --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
