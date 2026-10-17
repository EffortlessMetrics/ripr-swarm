
## Pending — rust_array_element_drop_exit (1)

Reason:
RIPR-SPEC-0225: non-plain Drop on element type cannot pin probed element (8084)

Command:
`cargo xtask goldens bless rust_array_element_drop_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
