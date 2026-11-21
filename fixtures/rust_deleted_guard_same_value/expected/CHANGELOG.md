# Golden Output Changes

## Pending — rust_deleted_guard_same_value (1)

Reason:
RIPR-SPEC-0197 (#14936): a deleted early return that gives the remaining tail value is not exposed; a differing tail stays exposed

Command:
`cargo xtask goldens bless rust_deleted_guard_same_value --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_deleted_guard_same_value (2)

Reason:
RIPR-SPEC-0122: removed-code-only runs name the static evidence on the removed code in Safe next action (#15335)

Command:
`cargo xtask goldens bless rust_deleted_guard_same_value --reason "..."`

Updated:
- `expected/human.txt`

## Pending — rust_deleted_guard_same_value (3)

Reason:
RIPR-SPEC-0122: a removal-only run lists the removed lines and what the static evidence on each shows under Safe next action (#15617)

Command:
`cargo xtask goldens bless rust_deleted_guard_same_value --reason "..."`

Updated:
- `expected/human.txt`
