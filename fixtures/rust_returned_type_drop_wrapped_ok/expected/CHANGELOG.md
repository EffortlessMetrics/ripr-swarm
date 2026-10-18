# Golden Output Changes

## Pending — rust_returned_type_drop_wrapped_ok (1)

Reason:
RIPR-SPEC-0197: a non-plain Drop record wrapped in Ok in exit position still demotes to weakly_exposed

Command:
`cargo xtask goldens bless rust_returned_type_drop_wrapped_ok --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_returned_type_drop_wrapped_ok (2)

Reason:
RIPR-SPEC-0197: a non-plain Drop record wrapped in Ok in exit position still demotes to weakly_exposed

Command:
`cargo xtask goldens bless rust_returned_type_drop_wrapped_ok --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
