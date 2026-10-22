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
RIPR-SPEC-0247 rule 5 (#9532): a truthiness assertion on a literal call to a boolean-declared owner is an exact pin, so the live toBeTruthy reads exact_value instead of smoke_only; verdict state unchanged except typescript_jest_vitest_assertion_facts isReady(1), which now credits the boundary pin

Command:
`cargo xtask goldens bless ts_dead_assertion_after_try_empty_catch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
