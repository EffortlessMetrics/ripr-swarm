# Golden Output Changes

## Pending — python_round_digits_no_exposed_credit (1)

Reason:
RIPR-SPEC-0028: a round digit count edit whose related calls stay within the smaller count reads weakly_exposed (#16203)

Command:
`cargo xtask goldens bless python_round_digits_no_exposed_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
