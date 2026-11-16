# Golden Output Changes

## Pending — rust_guarded_boundary_guard_missing (1)

Reason:
RIPR-SPEC-0001: a guarded boundary row that misses its guard reads infection_unknown with the guard named, not exposed through the compound reading (#13037)

Command:
`cargo xtask goldens bless rust_guarded_boundary_guard_missing --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_guarded_boundary_guard_missing (2)

Reason:
RIPR-SPEC-0001: add the human-full projection of the guarded-boundary refusal (#13037)

Command:
`cargo xtask goldens bless rust_guarded_boundary_guard_missing --reason "..."`

Updated:
- `expected/human-full.txt`
