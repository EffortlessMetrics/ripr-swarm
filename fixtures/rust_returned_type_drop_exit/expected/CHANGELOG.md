
## Pending — rust_returned_type_drop_exit (1)

Reason:
RIPR-SPEC-0197: non-plain Drop on returned type cannot pin constructed field (8480)

Command:
`cargo xtask goldens bless rust_returned_type_drop_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
