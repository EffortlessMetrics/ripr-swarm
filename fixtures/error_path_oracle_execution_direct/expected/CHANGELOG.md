# Golden Output Changes

## Pending — error_path_oracle_execution_direct (1)

Reason:
RIPR-SPEC-0197 #5027: matched equality execution admission; direct/invoked positives and ineffective/removal negatives are independently pinned by compiled runtime controls and the family-selected honesty corpus.

Command:
`cargo xtask goldens bless error_path_oracle_execution_direct --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — error_path_oracle_execution_direct (2)

Reason:
RIPR-SPEC-0197 #5027: retain JSON/human/full projections of independently compiled effective and ineffective equality controls; shared execution provenance and static confidence agree.

Command:
`cargo xtask goldens bless error_path_oracle_execution_direct --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_oracle_execution_direct (3)

Reason:
RIPR-SPEC-0197 #5027: retain JSON/human/full projections of independently compiled effective and ineffective equality controls; shared execution provenance and static confidence agree.

Command:
`cargo xtask goldens bless error_path_oracle_execution_direct --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_oracle_execution_direct (4)

Reason:
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless error_path_oracle_execution_direct --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
