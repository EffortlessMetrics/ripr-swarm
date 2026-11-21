# Golden Output Changes

## Pending — rust_returned_type_drop_macro_exit (1)

Reason:
RIPR-SPEC-0197: macro-written or renamed returned-type Drop impl read by the owner return pin (#15107)

Command:
`cargo xtask goldens bless rust_returned_type_drop_macro_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_returned_type_drop_macro_exit (2)

Reason:
RIPR-SPEC-0197 For rust_returned_type_drop_macro_exit, admit the discriminator only with assertion evidence linked to the changed result.

Command:
`cargo xtask goldens bless rust_returned_type_drop_macro_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human-full.txt`
