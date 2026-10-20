# Golden Output Changes

## Pending — rust_returned_type_drop_local_guard (1)

Reason:
RIPR-SPEC-0197: a non-plain Drop record built only in a local non-returned position does not demote the scalar return_value finding

Command:
`cargo xtask goldens bless rust_returned_type_drop_local_guard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_returned_type_drop_local_guard (2)

Reason:
RIPR-SPEC-0197: a non-plain Drop record built only in a local non-returned position does not demote the scalar return_value finding

Command:
`cargo xtask goldens bless rust_returned_type_drop_local_guard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_returned_type_drop_local_guard (3)

Reason:
RIPR-SPEC-0197: local guard Drop body is now a benign non-empty update instead of a process exit, so the fixture isolates the exit-position claim; expected output refreshed from this LF tree

Command:
`cargo xtask goldens bless rust_returned_type_drop_local_guard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_returned_type_drop_local_guard (4)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless rust_returned_type_drop_local_guard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_returned_type_drop_local_guard (5)

Reason:
RIPR-SPEC-0224: ungraded oracle reads '<kind> oracle, not graded against this probe' (#8388)

Command:
`cargo xtask goldens bless rust_returned_type_drop_local_guard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
