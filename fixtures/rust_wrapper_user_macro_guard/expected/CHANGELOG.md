# Golden Output Changes

## Pending — rust_wrapper_user_macro_guard (1)

Reason:
RIPR-SPEC-0186 #10284: a bare user macro named like a logging macro may return early; neither twin is promoted

Command:
`cargo xtask goldens bless rust_wrapper_user_macro_guard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_wrapper_user_macro_guard (2)

Reason:
RIPR-SPEC-0186 #10284: add human-full surface for the honesty corpus expected_class assertion

Command:
`cargo xtask goldens bless rust_wrapper_user_macro_guard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
