
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

## Pending — rust_returned_type_drop_exit (3)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless rust_returned_type_drop_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_returned_type_drop_exit (4)

Reason:
RIPR-SPEC-0259 and RIPR-SPEC-0094 #8954: condition return advice on return sensitivity; preserve actual effect versus return evidence and authentic standalone call identity.

Command:
`cargo xtask goldens bless rust_returned_type_drop_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
