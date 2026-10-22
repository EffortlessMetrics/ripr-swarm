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
RIPR-SPEC-0224: TypeScript weakly_exposed related-test rows carry the row-local miss and why (#5495)

Command:
`cargo xtask goldens bless ts_dead_assertion_after_try_empty_catch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
