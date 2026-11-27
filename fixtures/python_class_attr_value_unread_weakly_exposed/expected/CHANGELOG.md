# Golden Output Changes

## Pending — python_class_attr_value_unread_weakly_exposed (1)

Reason:
RIPR-SPEC-0238: #16009 an unread non-string class attribute value reads weakly exposed

Command:
`cargo xtask goldens bless python_class_attr_value_unread_weakly_exposed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
