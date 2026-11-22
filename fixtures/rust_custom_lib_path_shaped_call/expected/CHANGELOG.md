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
RIPR-SPEC-0172: composed with the public-source import, the directory-shaped call stays weakly_exposed and the strongest-oracle identity gate names it (identity_unresolved) before the pairing summary

Command:
`cargo xtask goldens bless rust_custom_lib_path_shaped_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
