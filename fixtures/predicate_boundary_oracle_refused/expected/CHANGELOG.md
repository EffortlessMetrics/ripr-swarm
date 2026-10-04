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
RIPR-SPEC-0122: #5471 check prints the agent stub route only when the stub resolver produces a stub; this route was refused or found no gap, so it is replaced by the refusal reason or removed

Command:
`cargo xtask goldens bless predicate_boundary_oracle_refused --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — predicate_boundary_oracle_refused (4)

Reason:
RIPR-SPEC-0122: #5471 agent stub --at resolves on the single-file seam shapes check already judged a gap, without re-classifying, so this route now yields a stub or a different refusal

Command:
`cargo xtask goldens bless predicate_boundary_oracle_refused --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — predicate_boundary_oracle_refused (5)

Reason:
RIPR-SPEC-0122: #5471 the stub route carries the finding probe family as --kind, and seams of that kind are tried first; refusals name that seam

Command:
`cargo xtask goldens bless predicate_boundary_oracle_refused --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
