# Golden Output Changes

## Pending — error_path_oracle_execution_false_branch (1)

Reason:
RIPR-SPEC-0197 #5027: matched equality execution admission; direct/invoked positives and ineffective/removal negatives are independently pinned by compiled runtime controls and the family-selected honesty corpus.

Command:
`cargo xtask goldens bless error_path_oracle_execution_false_branch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — error_path_oracle_execution_false_branch (2)

Reason:
RIPR-SPEC-0197 #5027: retain JSON/human/full projections of independently compiled effective and ineffective equality controls; shared execution provenance and static confidence agree.

Command:
`cargo xtask goldens bless error_path_oracle_execution_false_branch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_oracle_execution_false_branch (3)

Reason:
RIPR-SPEC-0197 #5027: retain JSON/human/full projections of independently compiled effective and ineffective equality controls; shared execution provenance and static confidence agree.

Command:
`cargo xtask goldens bless error_path_oracle_execution_false_branch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_oracle_execution_false_branch (4)

Reason:
RIPR-SPEC-0122: check names the one-step ripr agent stub route for Rust value gaps (#5355)

Command:
`cargo xtask goldens bless error_path_oracle_execution_false_branch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
## Pending — error_path_oracle_execution_false_branch (5)

Reason:
RIPR-SPEC-0224: related tests ripr examined stay listed and each names why it misses the change (#5344); verdicts unchanged

Command:
`cargo xtask goldens bless error_path_oracle_execution_false_branch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_oracle_execution_false_branch (6)

Reason:
RIPR-SPEC-0224: a matched related test keeps its oracle kind and strength in full output and adds why it still misses; verdicts unchanged

Command:
`cargo xtask goldens bless error_path_oracle_execution_false_branch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_oracle_execution_false_branch (7)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless error_path_oracle_execution_false_branch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_oracle_execution_false_branch (8)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless error_path_oracle_execution_false_branch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_oracle_execution_false_branch (9)

Reason:
RIPR-SPEC-0002: merge of origin/main (#5268 canonical_gap for Rust and sibling updates) onto the #5996 workspace-relative location owner — expected files regenerate through the merged tree

Command:
`cargo xtask goldens bless error_path_oracle_execution_false_branch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_oracle_execution_false_branch (10)

Reason:
RIPR-SPEC-0197: re-apply #5359 refusal disclosure (Why unrevealed / Not credited / owner-calling next step) on top of main's workspace-relative locations and canonical gap lines after merging main

Command:
`cargo xtask goldens bless error_path_oracle_execution_false_branch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_oracle_execution_false_branch (11)

Reason:
RIPR-SPEC-0116: the check drill-in binds --root to the resolved repository (#3948); goldens carry the <cwd>/ placeholder

Command:
`cargo xtask goldens bless error_path_oracle_execution_false_branch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_oracle_execution_false_branch (12)

Reason:
RIPR-SPEC-0240: a non-limit assertion refusal no longer offers a static-limit reading (#6903)

Command:
`cargo xtask goldens bless error_path_oracle_execution_false_branch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_oracle_execution_false_branch (13)

Reason:
RIPR-SPEC-0122 #5471: stub route printed only when the resolver yields a stub, with --kind; refusal or nothing otherwise (merge re-bless)

Command:
`cargo xtask goldens bless error_path_oracle_execution_false_branch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
