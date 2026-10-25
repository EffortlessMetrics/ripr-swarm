# Golden Output Changes

## Pending — binding_value_semantics_fail_closed (1)

Reason:
Native Rust and public API red/green controls reject signed host-index folding and skipped eager defaults; preserve unknown predicate boundaries under RIPR-SPEC-0158.

Command:
`cargo xtask goldens bless binding_value_semantics_fail_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — binding_value_semantics_fail_closed (2)

Reason:
RIPR-SPEC-0158: retain full human output alongside JSON so the independent honesty corpus checks both projections of the two unknown predicate boundaries.

Command:
`cargo xtask goldens bless binding_value_semantics_fail_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
