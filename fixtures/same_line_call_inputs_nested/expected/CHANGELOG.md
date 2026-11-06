# Golden Output Changes

## Pending — same_line_call_inputs_nested (1)

Reason:
RIPR-SPEC-0001: nested input is not an observed outer return for issue 13373

Command:
`cargo xtask goldens bless same_line_call_inputs_nested --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — same_line_call_inputs_nested (2)

Reason:
RIPR-SPEC-0001 / RIPR-SPEC-0186 / RIPR-SPEC-0197 and accepted main #13400: retain unobservable masked/nested/custom-Not predicate refusals and the honest strongest-oracle identity explanation; refresh the specifically reviewed full native output on frozen 7ce1289b8baf8837af5e0ed880363af6687305e0.

Command:
`cargo xtask goldens bless same_line_call_inputs_nested --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
