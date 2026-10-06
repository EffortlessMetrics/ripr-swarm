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
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless predicate_boundary_oracle_admitted --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
