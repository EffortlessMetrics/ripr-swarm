# Golden Output Changes

## Pending — rust_copy_builder_field_write (1)

Reason:
RIPR-SPEC-0228: initial golden; a by-value receiver's field write is not confirmed by a read of the caller's unmoved binding (#12061)

Command:
`cargo xtask goldens bless rust_copy_builder_field_write --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_copy_builder_field_write (2)

Reason:
RIPR-SPEC-0228: a by-value receiver's field write is refused with the by-value reason and told to assert on the call's result (#12061)

Command:
`cargo xtask goldens bless rust_copy_builder_field_write --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
