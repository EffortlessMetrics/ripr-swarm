# Golden Output Changes

## Pending — same_line_call_inputs (1)

Reason:
RIPR-SPEC-0001: retain each same-line free-owner input for issue 13373

Command:
`cargo xtask goldens bless same_line_call_inputs --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — same_line_call_inputs (2)

Reason:
RIPR-SPEC-0422: additive evidence_stages rows (#13443); fixture landed after the 905-fixture bless, main-health recovery

Command:
`cargo xtask goldens bless same_line_call_inputs --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — same_line_call_inputs (3)

Reason:
RIPR-SPEC-0122: the all-clear Start here says no test gap was found and that nothing is needed (#14080); human text only, check.json unchanged

Command:
`cargo xtask goldens bless same_line_call_inputs --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — same_line_call_inputs (4)

Reason:
RIPR-SPEC-0197: the edit changes the inline branch value 50 to 60 and leaves weight > 100 untouched, so the finding is the return value 60, which the same-line pin shipping(101) == 50 exposes (#15529)

Command:
`cargo xtask goldens bless same_line_call_inputs --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
