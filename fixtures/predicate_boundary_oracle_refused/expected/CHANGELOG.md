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
RIPR-SPEC-0197: a refused assert_eq! discloses why it was not credited; a refused context no longer claims no assertion or oracle was detected

Command:
`cargo xtask goldens bless predicate_boundary_oracle_refused --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
