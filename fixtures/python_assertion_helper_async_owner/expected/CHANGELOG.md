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

## Pending — python_assertion_helper_async_owner (3)

Reason:
RIPR-SPEC-0122: #9373 item 1 bounded Evidence legend on new current-main fixture; preserve current JSON, human-full and discriminator facts

Command:
`cargo xtask goldens bless python_assertion_helper_async_owner --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_assertion_helper_async_owner (4)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`
