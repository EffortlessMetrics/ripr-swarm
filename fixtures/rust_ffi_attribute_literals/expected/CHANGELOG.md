# Golden Output Changes

## Pending — rust_ffi_attribute_literals (1)

Reason:
RIPR-SPEC-0062: raw doc strings and comments are not FFI attributes; preserve the real unsafe(no_mangle) after raw documentation. Three public check_workspace controls fail before and pass after the lexical split repair.

Command:
`cargo xtask goldens bless rust_ffi_attribute_literals --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_ffi_attribute_literals (2)

Reason:
RIPR-SPEC-0002: #7755 double-colon fix (language counts use =); #7756 labelled human lines carry no terminal period

Command:
`cargo xtask goldens bless rust_ffi_attribute_literals --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_ffi_attribute_literals (3)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless rust_ffi_attribute_literals --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
