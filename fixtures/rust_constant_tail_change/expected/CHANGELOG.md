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

## Pending — rust_constant_tail_change (4)

Reason:
RIPR-SPEC-0122: the top-gap Safe next action points at the Next step the digest prints (#14173)

Command:
`cargo xtask goldens bless rust_constant_tail_change --reason "..."`

Updated:
- `expected/human.txt`

## Pending — rust_constant_tail_change (5)

Reason:
RIPR-SPEC-0197: off() rewrites false as !true, the same value, so the line seeds no probe (#14938)

Command:
`cargo xtask goldens bless rust_constant_tail_change --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_constant_tail_change (6)

Reason:
RIPR-SPEC-0001: #7807 a removed line read as static_unknown only for lack of context is not reported again when its block replacement's modeled probe carries the whole removed text as before

Command:
`cargo xtask goldens bless rust_constant_tail_change --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_constant_tail_change (7)

Reason:
RIPR-SPEC-0197 The separate base-side return in pick has no discriminating assertion. Its existing test observes an empty vector on both sides.

Command:
`cargo xtask goldens bless rust_constant_tail_change --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
