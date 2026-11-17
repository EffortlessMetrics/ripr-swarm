# Golden Output Changes

## Pending — rust_guard_return_changed_guard (1)

Reason:
RIPR-SPEC-0197: a guard-return pin stays weak when the diff also changes the guard condition (#14387)

Command:
`cargo xtask goldens bless rust_guard_return_changed_guard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_guard_return_changed_guard (2)

Reason:
RIPR-SPEC-0197: add the human-full projection of the changed-guard refusal (#14387)

Command:
`cargo xtask goldens bless rust_guard_return_changed_guard --reason "..."`

Updated:
- `expected/human-full.txt`
