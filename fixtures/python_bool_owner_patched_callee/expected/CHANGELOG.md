# Golden Output Changes

## Pending — python_bool_owner_patched_callee (1)

Reason:
RIPR-SPEC-0028: a bare-bool assert in a file that patches its callee by name stays weakly_exposed (#10391)

Command:
`cargo xtask goldens bless python_bool_owner_patched_callee --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_bool_owner_patched_callee (2)

Reason:
RIPR-SPEC-0028: Linux-hosted rendering refresh for the patch-scope fixtures; hosted Rust-small gate observed formatting_only drift on the human surfaces

Command:
`cargo xtask goldens bless python_bool_owner_patched_callee --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
