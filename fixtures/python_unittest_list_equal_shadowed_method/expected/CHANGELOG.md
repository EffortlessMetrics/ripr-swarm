# Golden Output Changes

## Pending — python_unittest_list_equal_shadowed_method (1)

Reason:
RIPR-SPEC-0028: duplicate unittest methods shadow the earlier assertion

Command:
`cargo xtask goldens bless python_unittest_list_equal_shadowed_method --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_unittest_list_equal_shadowed_method (2)

Reason:
RIPR-SPEC-0028: pin full human output for the shadowed unittest assertion

Command:
`cargo xtask goldens bless python_unittest_list_equal_shadowed_method --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
