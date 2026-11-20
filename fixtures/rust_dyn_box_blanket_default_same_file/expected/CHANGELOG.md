# Golden Output Changes

## Pending — rust_dyn_box_blanket_default_same_file (1)

Reason:
RIPR-SPEC-0001: a Box<dyn Trait> call is not credited when an unread source or another same-name method may answer it at the Box (#15277)

Command:
`cargo xtask goldens bless rust_dyn_box_blanket_default_same_file --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
