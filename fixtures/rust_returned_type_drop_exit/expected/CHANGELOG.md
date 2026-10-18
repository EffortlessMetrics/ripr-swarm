
## Pending — rust_returned_type_drop_exit (1)

Reason:
RIPR-SPEC-0197: non-plain Drop on returned type cannot pin constructed field (8480)

Command:
`cargo xtask goldens bless rust_returned_type_drop_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_returned_type_drop_exit (2)

Reason:
RIPR-SPEC-0197: return_value demotes with a non-plain Drop on the returned type, matching the field_construction refusal

Command:
`cargo xtask goldens bless rust_returned_type_drop_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
