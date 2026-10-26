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

## Pending — ts_dead_assertion_after_try_empty_catch (4)

Reason:
RIPR-SPEC-0224: TypeScript row-miss narrowed to proven paths (#5495)

Command:
`cargo xtask goldens bless ts_dead_assertion_after_try_empty_catch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — ts_dead_assertion_after_try_empty_catch (5)

Reason:
RIPR-SPEC-0247 rule 5 (#9532): a truthiness assertion on a literal call to a boolean-declared owner is an exact pin, so the live toBeTruthy reads exact_value instead of smoke_only; verdict state unchanged except typescript_jest_vitest_assertion_facts isReady(1), which now credits the boundary pin

Command:
`cargo xtask goldens bless ts_dead_assertion_after_try_empty_catch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — ts_dead_assertion_after_try_empty_catch (6)

Reason:
RIPR-SPEC-0224 merge of main into #5495: TypeScript row carries observation_unconfirmed (strong-path oracle after #9639/#9777 credit); only the miss field and its rendering change

Command:
`cargo xtask goldens bless ts_dead_assertion_after_try_empty_catch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — ts_dead_assertion_after_try_empty_catch (7)

Reason:
RIPR-SPEC-0122: explain bounded Evidence stage and state meanings for #9373 item 1; retain full-human and JSON contracts

Command:
`cargo xtask goldens bless ts_dead_assertion_after_try_empty_catch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — ts_dead_assertion_after_try_empty_catch (8)

Reason:
RIPR-SPEC-0122: #9373 item 1 review repair; missing discriminators include absent assertions as well as inputs

Command:
`cargo xtask goldens bless ts_dead_assertion_after_try_empty_catch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — ts_dead_assertion_after_try_empty_catch (9)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — ts_dead_assertion_after_try_empty_catch (10)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless ts_dead_assertion_after_try_empty_catch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — ts_dead_assertion_after_try_empty_catch (11)

Reason:
RIPR-SPEC-0122: digest missing-discriminator prose wraps instead of one_line clipping (#4323 parity); the ellipsized cut dropped the operative tail silently

Command:
`cargo xtask goldens bless ts_dead_assertion_after_try_empty_catch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — ts_dead_assertion_after_try_empty_catch (12)

Reason:
RIPR-SPEC-0122: name the no-canonical-gap recovery instead of routing back to the gate (#9244)

Command:
`cargo xtask goldens bless ts_dead_assertion_after_try_empty_catch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — ts_dead_assertion_after_try_empty_catch (13)

Reason:
RIPR-SPEC-0122: state only that the finding has no canonical gap, without a discriminator claim (#9244)

Command:
`cargo xtask goldens bless ts_dead_assertion_after_try_empty_catch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — ts_dead_assertion_after_try_empty_catch (14)

Reason:
RIPR-SPEC-0122: define finding, unsuppressed, unknown, base-side and candidate line in the evidence legend (#9244)

Command:
`cargo xtask goldens bless ts_dead_assertion_after_try_empty_catch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
