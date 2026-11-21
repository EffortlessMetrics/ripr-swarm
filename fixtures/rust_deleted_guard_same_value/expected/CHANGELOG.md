# Golden Output Changes

## Pending — rust_deleted_guard_same_value (1)

Reason:
RIPR-SPEC-0197 (#14936): a deleted early return that gives the remaining tail value is not exposed; a differing tail stays exposed

Command:
`cargo xtask goldens bless rust_deleted_guard_same_value --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
