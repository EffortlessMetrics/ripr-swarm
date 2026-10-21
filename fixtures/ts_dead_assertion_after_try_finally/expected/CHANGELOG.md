# Golden Output Changes

## Pending — ts_dead_assertion_after_try_finally (1)

Reason:
RIPR-SPEC-0243: new #9138 fixture, assertion after an always-exiting try/finally is dead and stays weakly_exposed

Command:
`cargo xtask goldens bless ts_dead_assertion_after_try_finally --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_dead_assertion_after_try_finally (2)

Reason:
RIPR-SPEC-0243: new #9138 fixture, assertion after an always-exiting try/finally is dead and stays weakly_exposed

Command:
`cargo xtask goldens bless ts_dead_assertion_after_try_finally --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — ts_dead_assertion_after_try_finally (3)

Reason:
RIPR-SPEC-0243: main #8711 (oracle_phrase) dedups the repeated 'smoke smoke only' strength in related-test phrases; regenerate the #9138 fixture golden against merged main

Command:
`cargo xtask goldens bless ts_dead_assertion_after_try_finally --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
