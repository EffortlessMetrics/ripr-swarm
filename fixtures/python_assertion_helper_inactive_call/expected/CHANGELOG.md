# Golden Output Changes

## Pending — python_assertion_helper_inactive_call (1)

Reason:
RIPR-SPEC-0238: same-file helper credit requires the executed changed field and receiver; runtime deletion controls pin surviving negatives (#9594)

Command:
`cargo xtask goldens bless python_assertion_helper_inactive_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_assertion_helper_inactive_call (2)

Reason:
RIPR-SPEC-0238: retain full human output for the RIPR-SPEC-0108 independent expected-class invariant on the helper control (#9594)

Command:
`cargo xtask goldens bless python_assertion_helper_inactive_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
