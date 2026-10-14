# Golden Output Changes

## Pending — python_unittest_list_equal_equal_values (1)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: native assertions accept both [1, 1] and [1, True]; independently require weakly_exposed, strong exact_value and weak discrimination, never promotion.

Command:
`cargo xtask goldens bless python_unittest_list_equal_equal_values --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_unittest_list_equal_equal_values (2)

Reason:
RIPR-SPEC-0028: keep weakly_exposed warning and strong oracle, withhold ungrounded exact-assertion repair guidance when equality cannot establish different values; real CLI independently asserts no typed discriminator, placement, card or strengthen next step.

Command:
`cargo xtask goldens bless python_unittest_list_equal_equal_values --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
