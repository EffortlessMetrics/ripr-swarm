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
