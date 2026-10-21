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
RIPR-SPEC-0224: unconfirmed-observation wording from #5578 reaches the fixture #5255 added with the old miss text
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

## Pending — error_path_operand_error_lexeme (9)

Reason:
RIPR-SPEC-0003: #7477 gives reason-specific uncertainty-resolution actions without verdict-change promises or unsupported deep/fixture-config routes; classifications, evidence, and decidable guidance unchanged.

Command:
`cargo xtask goldens bless error_path_operand_error_lexeme --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_operand_error_lexeme (10)

Reason:
RIPR-SPEC-0003: #7477 conditional assertion inspection and actual configured runner guidance; whole JSON evidence and classifications independently preserved.

Command:
`cargo xtask goldens bless error_path_operand_error_lexeme --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_operand_error_lexeme (11)

Reason:
RIPR-SPEC-0259: Next step for error_path/return_value weakly_exposed findings names the missing input when the strongest related oracle is already exact (#7393); guidance text only, no class or evidence change

Command:
`cargo xtask goldens bless error_path_operand_error_lexeme --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_operand_error_lexeme (12)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless error_path_operand_error_lexeme --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_operand_error_lexeme (13)

Reason:
RIPR-SPEC-0001: output contract changed - 7798 collapse tautological unknown-sink flow row when sink text restates the kind label

Command:
`cargo xtask goldens bless error_path_operand_error_lexeme --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_operand_error_lexeme (14)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless error_path_operand_error_lexeme --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_operand_error_lexeme (15)

Reason:
RIPR-SPEC-0122: #8389 Summary names base-side findings, Hidden total equals listed bullets, all-unknown footer no longer claims a found absence

Command:
`cargo xtask goldens bless error_path_operand_error_lexeme --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_operand_error_lexeme (16)

Reason:
RIPR-SPEC-0268: human evidence window glosses the unknown-sink row with its meaning (#8744)

Command:
`cargo xtask goldens bless error_path_operand_error_lexeme --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
## Pending — error_path_operand_error_lexeme (17)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless error_path_operand_error_lexeme --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
## Pending — error_path_operand_error_lexeme (18)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless error_path_operand_error_lexeme --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_operand_error_lexeme (17)

Reason:
RIPR-SPEC-0224: ungraded oracle reads '<kind> oracle, not graded against this probe' (#8388)

Command:
`cargo xtask goldens bless error_path_operand_error_lexeme --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_operand_error_lexeme (19)

Reason:
RIPR-SPEC-0284: unknown-sink row gloss from the context-packet clarity lane lands on this golden after merging main

Command:
`cargo xtask goldens bless error_path_operand_error_lexeme --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
