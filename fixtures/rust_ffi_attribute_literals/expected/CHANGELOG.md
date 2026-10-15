# Golden Output Changes

## Pending — rust_ffi_attribute_literals (1)

Reason:
RIPR-SPEC-0062: raw doc strings and comments are not FFI attributes; preserve the real unsafe(no_mangle) after raw documentation. Three public check_workspace controls fail before and pass after the lexical split repair.

Command:
`cargo xtask goldens bless rust_ffi_attribute_literals --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
