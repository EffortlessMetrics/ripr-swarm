# Golden Output Changes

## Pending — rust_where_bound_declarations (1)

Reason:
RIPR-SPEC-0001: issue #7251 bound keeps a static limitation while the real uppercase runtime field remains executable

Command:
`cargo xtask goldens bless rust_where_bound_declarations --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_where_bound_declarations (2)

Reason:
RIPR-SPEC-0001: issue #7251 bound retains static limitation in JSON and full human projections

Command:
`cargo xtask goldens bless rust_where_bound_declarations --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
