# Golden Output Changes

## Pending — oracle_confirmation_mixed (1)

Reason:
RIPR-SPEC-0094 (#4404): unrelated strongest exact oracle must not borrow weaker token confirmation; valid before/head/call-removal variants each pass two tests.

Command:
`cargo xtask goldens bless oracle_confirmation_mixed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — oracle_confirmation_mixed (2)

Reason:
RIPR-SPEC-0122: human-full drill-in commands (#4411) now appear on the #4421 fixture

Command:
`cargo xtask goldens bless oracle_confirmation_mixed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
