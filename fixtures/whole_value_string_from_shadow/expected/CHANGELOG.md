# Golden Output Changes

## Pending — whole_value_string_from_shadow (1)

Reason:
RIPR-SPEC-0225: #7066 review control - a test-file mod String shadows the String::from conversion, so the whole-value field pin must refuse it and the finding stays weakly_exposed

Command:
`cargo xtask goldens bless whole_value_string_from_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — whole_value_string_from_shadow (2)

Reason:
RIPR-SPEC-0225: #7066 review control - a test-file mod String shadows the String::from conversion, so the whole-value field pin must refuse it and the finding stays weakly_exposed

Command:
`cargo xtask goldens bless whole_value_string_from_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
