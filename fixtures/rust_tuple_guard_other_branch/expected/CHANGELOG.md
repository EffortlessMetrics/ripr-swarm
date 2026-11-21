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

## Pending — rust_tuple_guard_other_branch (2)

Reason:
RIPR-SPEC-0443: history-sync composition; the import line's delta-token rule leaves the exact assertion unconfirmed for the changed expression; class stays weakly_exposed with lower confidence

Command:
`cargo xtask goldens bless rust_tuple_guard_other_branch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
