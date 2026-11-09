# Golden Output Changes

## Pending — ts_multiline_return_continuation_clause (1)

Reason:
RIPR-SPEC-0027: a typeof clause of a multi-line return, conditional arm or returned-object property is judged on its statement-level expression, not the changed line alone (#12380)

Command:
`cargo xtask goldens bless ts_multiline_return_continuation_clause --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_multiline_return_continuation_clause (2)

Reason:
RIPR-SPEC-0423: additive evidence_stages rows and agent stages= line; human rendering unchanged

Command:
`cargo xtask goldens bless ts_multiline_return_continuation_clause --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_multiline_return_continuation_clause (3)

Reason:
RIPR-SPEC-0423: additive evidence_stages rows and agent stages= line; no test attribution; human rendering unchanged

Command:
`cargo xtask goldens bless ts_multiline_return_continuation_clause --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
