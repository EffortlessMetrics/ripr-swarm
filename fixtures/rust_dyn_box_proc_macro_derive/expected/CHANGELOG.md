# Golden Output Changes

## Pending — rust_dyn_box_proc_macro_derive (1)

Reason:
RIPR-SPEC-0001: a Box<dyn Trait> call is not credited when an unread source or another same-name method may answer it at the Box (#15277)

Command:
`cargo xtask goldens bless rust_dyn_box_proc_macro_derive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_dyn_box_proc_macro_derive (2)

Reason:
RIPR-SPEC-0001: rename the fixture member so default output avoids an internal term (#15277)

Command:
`cargo xtask goldens bless rust_dyn_box_proc_macro_derive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
