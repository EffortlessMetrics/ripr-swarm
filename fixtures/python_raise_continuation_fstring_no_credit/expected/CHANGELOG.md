# Golden Output Changes

## Pending — python_raise_continuation_fstring_no_credit (1)

Reason:
RIPR-SPEC-0108: an f-string message on a multi-line raise's continuation line stays weakly_exposed under success-only exact tests (#15246)

Command:
`cargo xtask goldens bless python_raise_continuation_fstring_no_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
