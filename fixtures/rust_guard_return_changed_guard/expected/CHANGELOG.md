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

## Pending — rust_guard_return_changed_guard (3)

Reason:
RIPR-SPEC-0122: the top-gap Safe next action points at the Next step the digest prints (#14173)

Command:
`cargo xtask goldens bless rust_guard_return_changed_guard --reason "..."`

Updated:
- `expected/human.txt`

## Pending — rust_guard_return_changed_guard (4)

Reason:
RIPR-SPEC-0122: #15313 Summary names shared candidate lines and says No test gap when every finding reads noticed

Command:
`cargo xtask goldens bless rust_guard_return_changed_guard --reason "..."`

Updated:
- `expected/human.txt`
