# Golden Output Changes

## Pending — ts_dead_assertion_after_try_empty_catch (1)

Reason:
RIPR-SPEC-0243: bare non-throwing first exit makes the empty catch unreachable so the later assertion is dead (#9150)

Command:
`cargo xtask goldens bless ts_dead_assertion_after_try_empty_catch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_dead_assertion_after_try_empty_catch (2)

Reason:
RIPR-SPEC-0243: add human-full golden for bare-exit try fixture (#9150)

Command:
`cargo xtask goldens bless ts_dead_assertion_after_try_empty_catch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — ts_dead_assertion_after_try_empty_catch (3)

Reason:
RIPR-SPEC-0122: explain bounded Evidence stage and state meanings for #9373 item 1; retain full-human and JSON contracts

Command:
`cargo xtask goldens bless ts_dead_assertion_after_try_empty_catch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — ts_dead_assertion_after_try_empty_catch (4)

Reason:
RIPR-SPEC-0122: #9373 item 1 review repair; missing discriminators include absent assertions as well as inputs

Command:
`cargo xtask goldens bless ts_dead_assertion_after_try_empty_catch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
