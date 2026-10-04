# Golden Output Changes

## Pending — predicate_oracle_execution_shadowed (1)

Reason:
RIPR-SPEC-0197 #5027: matched equality execution admission; direct/invoked positives and ineffective/removal negatives are independently pinned by compiled runtime controls and the family-selected honesty corpus.

Command:
`cargo xtask goldens bless predicate_oracle_execution_shadowed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — predicate_oracle_execution_shadowed (2)

Reason:
RIPR-SPEC-0197 #5027: retain JSON/human/full projections of independently compiled effective and ineffective equality controls; shared execution provenance and static confidence agree.

Command:
`cargo xtask goldens bless predicate_oracle_execution_shadowed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — predicate_oracle_execution_shadowed (3)

Reason:
RIPR-SPEC-0197 #5027: retain JSON/human/full projections of independently compiled effective and ineffective equality controls; shared execution provenance and static confidence agree.

Command:
`cargo xtask goldens bless predicate_oracle_execution_shadowed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — predicate_oracle_execution_shadowed (4)

Reason:
RIPR-SPEC-0122: check names the one-step ripr agent stub route for Rust value gaps (#5355)

Command:
`cargo xtask goldens bless predicate_oracle_execution_shadowed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — predicate_oracle_execution_shadowed (5)

Reason:
RIPR-SPEC-0122: #5471 check prints the agent stub route only when the stub resolver produces a stub; this route was refused or found no gap, so it is replaced by the refusal reason or removed

Command:
`cargo xtask goldens bless predicate_oracle_execution_shadowed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — predicate_oracle_execution_shadowed (6)

Reason:
RIPR-SPEC-0122: #5471 agent stub --at resolves on the single-file seam shapes check already judged a gap, without re-classifying, so this route now yields a stub or a different refusal

Command:
`cargo xtask goldens bless predicate_oracle_execution_shadowed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
