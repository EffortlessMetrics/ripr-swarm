# Golden Output Changes

## Pending — python_conditional_foreign_callable_write (1)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: executed unchanged test cannot observe this edited callable or value; pin structural identity and reachability refusal

Command:
`cargo xtask goldens bless python_conditional_foreign_callable_write --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
