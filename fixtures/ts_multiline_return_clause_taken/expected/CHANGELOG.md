# Golden Output Changes

## Pending — ts_multiline_return_clause_taken (1)

Reason:
RIPR-SPEC-0027: a typeof clause of a multi-line return, conditional arm or returned-object property is judged on its statement-level expression, not the changed line alone (#12380)

Command:
`cargo xtask goldens bless ts_multiline_return_clause_taken --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
