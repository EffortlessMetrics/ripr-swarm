# Golden Output Changes

## Pending — rust_constant_tail_change (1)

Reason:
RIPR-SPEC-0197 (#14852): returned constant replaced by a different constant; positive controls and counterexamples

Command:
`cargo xtask goldens bless rust_constant_tail_change --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
