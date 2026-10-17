# Golden Output Changes

## Pending — test_cast_to_integer_not_exact (1)

Reason:
RIPR-SPEC-0231 rule 1 (#8093): assert_eq!(u8::from(score(points) == 0), zero) reads relational_check/weak, not exact_value/strong

Command:
`cargo xtask goldens bless test_cast_to_integer_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — test_cast_to_integer_not_exact (2)

Reason:
RIPR-SPEC-0002: #7755 double-colon fix (language counts use =); #7756 labelled human lines carry no terminal period

Command:
`cargo xtask goldens bless test_cast_to_integer_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — test_cast_to_integer_not_exact (3)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless test_cast_to_integer_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — test_cast_to_integer_not_exact (4)

Reason:
RIPR-SPEC-0158 and #7805: additive source_file metadata on test-source values only; check.json lines otherwise byte-identical

Command:
`cargo xtask goldens bless test_cast_to_integer_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
