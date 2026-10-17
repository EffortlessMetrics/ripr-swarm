# Golden Output Changes

## Pending — python_unittest_list_equal_derived_result (1)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: independently reproduced derived-result assertion accepts both owner values; pin literal static_unknown, unknown oracle, and non-promotion.

Command:
`cargo xtask goldens bless python_unittest_list_equal_derived_result --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_unittest_list_equal_derived_result (2)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless python_unittest_list_equal_derived_result --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_unittest_list_equal_derived_result (3)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless python_unittest_list_equal_derived_result --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
