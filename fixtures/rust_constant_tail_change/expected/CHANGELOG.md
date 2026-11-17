# Golden Output Changes

## Pending — rust_constant_tail_change (1)

Reason:
RIPR-SPEC-0197 (#14852): returned constant replaced by a different constant; positive controls and counterexamples

Command:
`cargo xtask goldens bless rust_constant_tail_change --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_constant_tail_change (2)

Reason:
RIPR-SPEC-0197: adopt the #14780 unknown-class next-step hedge wording; classification, evidence and oracle fields unchanged

Command:
`cargo xtask goldens bless rust_constant_tail_change --reason "..."`

Updated:
- `expected/check.json`

## Pending — rust_constant_tail_change (3)

Reason:
RIPR-SPEC-0197: the -delta counterexample pinned at zero now reads infection_unknown through the merged #14815 unary-tail path

Command:
`cargo xtask goldens bless rust_constant_tail_change --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
