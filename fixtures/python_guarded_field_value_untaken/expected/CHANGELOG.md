# Golden Output Changes

## Pending — python_guarded_field_value_untaken (1)

Reason:
RIPR-SPEC-0260 / #13621: new fixture; the only test never enters total < 0, so the changed field value is weakly_exposed

Command:
`cargo xtask goldens bless python_guarded_field_value_untaken --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_guarded_field_value_untaken (2)

Reason:
RIPR-SPEC-0108 / #13621: produce the required full human classification projection for the new guarded field-value fixture

Command:
`cargo xtask goldens bless python_guarded_field_value_untaken --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
