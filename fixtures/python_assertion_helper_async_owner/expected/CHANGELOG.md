# Golden Output Changes

## Pending — python_assertion_helper_async_owner (1)

Reason:
RIPR-SPEC-0238: helper-derived field credit requires a synchronous non-generator owner; discarded deferred bodies and deleted-write controls retain weak exposure (#9594)

Command:
`cargo xtask goldens bless python_assertion_helper_async_owner --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_assertion_helper_async_owner (2)

Reason:
RIPR-SPEC-0238: retain main helper-call rendering after independent deferred-body class pins and runtime controls (#9419)

Command:
`cargo xtask goldens bless python_assertion_helper_async_owner --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
