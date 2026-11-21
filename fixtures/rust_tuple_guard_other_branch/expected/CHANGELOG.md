# Golden Output Changes

## Pending — rust_tuple_guard_other_branch (1)

Reason:
RIPR-SPEC-0443: the only input wraps 2, so the edited addition guarded by a.0 > 100 is skipped

Command:
`cargo xtask goldens bless rust_tuple_guard_other_branch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
