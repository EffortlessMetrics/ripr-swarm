# Golden Output Changes

## Pending — inequality_assertion_not_exact (1)

Reason:
RIPR-SPEC-0231 rule 1: assert_ne! reads relational_check/weak, not exact_value/strong

Command:
`cargo xtask goldens bless inequality_assertion_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — inequality_assertion_not_exact (2)

Reason:
RIPR-SPEC-0231: re-bless after merging main's output-format changes (canonical gap id, relative file paths); assert_ne! still reads relational_check/weak

Command:
`cargo xtask goldens bless inequality_assertion_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — inequality_assertion_not_exact (3)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless inequality_assertion_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — inequality_assertion_not_exact (4)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless inequality_assertion_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
