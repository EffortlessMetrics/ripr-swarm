# Golden Output Changes

## Pending — python_get_fallback_no_exposed_credit (1)

Reason:
RIPR-SPEC-0028: a get fallback edit whose related calls all hold the key reads weakly_exposed (#16203)

Command:
`cargo xtask goldens bless python_get_fallback_no_exposed_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
