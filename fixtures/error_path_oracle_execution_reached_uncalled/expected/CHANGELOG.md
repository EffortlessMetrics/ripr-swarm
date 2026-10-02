# Golden Output Changes

## Pending — error_path_oracle_execution_reached_uncalled (1)

Reason:
RIPR-SPEC-0197 #5027: matched equality execution admission; direct/invoked positives and ineffective/removal negatives are independently pinned by compiled runtime controls and the family-selected honesty corpus.

Command:
`cargo xtask goldens bless error_path_oracle_execution_reached_uncalled --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
