# Golden Output Changes

## Pending — rust_predicate_added_disjunct_sibling_field (1)

Reason:
RIPR-SPEC-0158: an added equality disjunct is the predicate boundary, and pairing refuses a sibling-field read of the boundary call (#14726)

Command:
`cargo xtask goldens bless rust_predicate_added_disjunct_sibling_field --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_predicate_added_disjunct_sibling_field (2)

Reason:
RIPR-SPEC-0158: an added equality disjunct is the predicate boundary, and pairing refuses a sibling-field read of the boundary call (#14726)

Command:
`cargo xtask goldens bless rust_predicate_added_disjunct_sibling_field --reason "..."`

Updated:
- `expected/human-full.txt`

## Pending — rust_predicate_added_disjunct_sibling_field (3)

Reason:
RIPR-SPEC-0158: pairing refuses a sibling-field read of the boundary binding inside a closure of another call (#14726)

Command:
`cargo xtask goldens bless rust_predicate_added_disjunct_sibling_field --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
