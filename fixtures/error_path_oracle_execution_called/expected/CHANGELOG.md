# Golden Output Changes

## Pending — error_path_oracle_execution_called (1)

Reason:
RIPR-SPEC-0197 #5027: matched equality execution admission; direct/invoked positives and ineffective/removal negatives are independently pinned by compiled runtime controls and the family-selected honesty corpus.

Command:
`cargo xtask goldens bless error_path_oracle_execution_called --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — error_path_oracle_execution_called (2)

Reason:
RIPR-SPEC-0197 #5027: retain JSON/human/full projections of independently compiled effective and ineffective equality controls; shared execution provenance and static confidence agree.

Command:
`cargo xtask goldens bless error_path_oracle_execution_called --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_oracle_execution_called (3)

Reason:
RIPR-SPEC-0197 #5027: retain JSON/human/full projections of independently compiled effective and ineffective equality controls; shared execution provenance and static confidence agree.

Command:
`cargo xtask goldens bless error_path_oracle_execution_called --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_oracle_execution_called (4)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless error_path_oracle_execution_called --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_oracle_execution_called (5)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless error_path_oracle_execution_called --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_oracle_execution_called (6)

Reason:
RIPR-SPEC-0002: merge of origin/main (#5268 canonical_gap for Rust and sibling updates) onto the #5996 workspace-relative location owner — expected files regenerate through the merged tree

Command:
`cargo xtask goldens bless error_path_oracle_execution_called --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_oracle_execution_called (7)

Reason:
RIPR-SPEC-0116: the check drill-in binds --root to the resolved repository (#3948); goldens carry the <cwd>/ placeholder

Command:
`cargo xtask goldens bless error_path_oracle_execution_called --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_oracle_execution_called (8)

Reason:
RIPR-SPEC-0003: #7477 gives reason-specific uncertainty-resolution actions without verdict-change promises or unsupported deep/fixture-config routes; classifications, evidence, and decidable guidance unchanged.

Command:
`cargo xtask goldens bless error_path_oracle_execution_called --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_oracle_execution_called (9)

Reason:
RIPR-SPEC-0003: #7477 conditional assertion inspection and actual configured runner guidance; whole JSON evidence and classifications independently preserved.

Command:
`cargo xtask goldens bless error_path_oracle_execution_called --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_oracle_execution_called (10)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless error_path_oracle_execution_called --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_oracle_execution_called (11)

Reason:
RIPR-SPEC-0001: output contract changed - 7798 collapse tautological unknown-sink flow row when sink text restates the kind label

Command:
`cargo xtask goldens bless error_path_oracle_execution_called --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_oracle_execution_called (12)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless error_path_oracle_execution_called --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_oracle_execution_called (13)

Reason:
RIPR-SPEC-0122: #8389 Summary names base-side findings, Hidden total equals listed bullets, all-unknown footer no longer claims a found absence

Command:
`cargo xtask goldens bless error_path_oracle_execution_called --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
