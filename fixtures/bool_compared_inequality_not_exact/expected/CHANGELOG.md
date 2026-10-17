# Golden Output Changes

## Pending — bool_compared_inequality_not_exact (1)

Reason:
RIPR-SPEC-0231 rule 1 (#7598): assert_eq!(score(2) != 0, true) reads relational_check/weak, not exact_value/strong

Command:
`cargo xtask goldens bless bool_compared_inequality_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — bool_compared_inequality_not_exact (2)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless bool_compared_inequality_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — bool_compared_inequality_not_exact (3)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless bool_compared_inequality_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — bool_compared_inequality_not_exact (4)

Reason:
RIPR-SPEC-0158 and #7805: additive source_file metadata on test-source values only; check.json lines otherwise byte-identical

Command:
`cargo xtask goldens bless bool_compared_inequality_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
