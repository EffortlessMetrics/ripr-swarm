# Golden Output Changes

## Pending — rust_custom_lib_path_shaped_call (1)

Reason:
RIPR-SPEC-0172: a directory-shaped call under a [lib] path root does not name the owner (#14495)

Command:
`cargo xtask goldens bless rust_custom_lib_path_shaped_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_custom_lib_path_shaped_call (2)

Reason:
RIPR-SPEC-0172: on the composed history-sync tree the directory-shaped call stays weakly_exposed; the import line's #12010 identity rule names why (no sink-level identity for a path that names no module of this crate) instead of the generic unconfirmed-check line

Command:
`cargo xtask goldens bless rust_custom_lib_path_shaped_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
