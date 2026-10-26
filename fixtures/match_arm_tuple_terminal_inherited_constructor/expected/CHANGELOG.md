# Golden Output Changes

## Pending — match_arm_tuple_terminal_inherited_constructor (1)

Reason:
RIPR-SPEC-0093: retain the compiling crate-root Some constructor counterexample; the relation stays observation_unverified.

Command:
`cargo xtask goldens bless match_arm_tuple_terminal_inherited_constructor --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_tuple_terminal_inherited_constructor (2)

Reason:
RIPR-SPEC-0093: re-bless human output against current plain-language renderer (#8165)

Command:
`cargo xtask goldens bless match_arm_tuple_terminal_inherited_constructor --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_tuple_terminal_inherited_constructor (3)

Reason:
RIPR-SPEC-0093: re-bless human summary line with the base-side evidence suffix (stale after #7836 merged)

Command:
`cargo xtask goldens bless match_arm_tuple_terminal_inherited_constructor --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_tuple_terminal_inherited_constructor (4)

Reason:
RIPR-SPEC-0108: #7840 wrapped-signature parameter names now label observed function-argument values; classification unchanged

Command:
`cargo xtask goldens bless match_arm_tuple_terminal_inherited_constructor --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_tuple_terminal_inherited_constructor (5)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless match_arm_tuple_terminal_inherited_constructor --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_tuple_terminal_inherited_constructor (6)

Reason:
RIPR-SPEC-0122: human source-value evidence names the test file (path:line) only when the shown window spans more than one test file, and the window line reads 'read from related tests' (#8389)

Command:
`cargo xtask goldens bless match_arm_tuple_terminal_inherited_constructor --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_tuple_terminal_inherited_constructor (7)

Reason:
RIPR-SPEC-0152: base-side findings render at note severity in human output (#9004)

Command:
`cargo xtask goldens bless match_arm_tuple_terminal_inherited_constructor --reason "..."`

Updated:
- `expected/human-full.txt`

## Pending — match_arm_tuple_terminal_inherited_constructor (8)

Reason:
RIPR-SPEC-0122: human output drops the observation_unverified token

Command:
`cargo xtask goldens bless match_arm_tuple_terminal_inherited_constructor --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_tuple_terminal_inherited_constructor (9)

Reason:
RIPR-SPEC-0122: explain bounded Evidence stage and state meanings for #9373 item 1; retain full-human and JSON contracts

Command:
`cargo xtask goldens bless match_arm_tuple_terminal_inherited_constructor --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_tuple_terminal_inherited_constructor (10)

Reason:
RIPR-SPEC-0122: #9373 item 1 review repair; missing discriminators include absent assertions as well as inputs

Command:
`cargo xtask goldens bless match_arm_tuple_terminal_inherited_constructor --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_tuple_terminal_inherited_constructor (11)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — match_arm_tuple_terminal_inherited_constructor (12)

Reason:
RIPR-SPEC-0122: a head-only match-arm after row tells the reader to open the arm at its head line to read the new body (#9244)

Command:
`cargo xtask goldens bless match_arm_tuple_terminal_inherited_constructor --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_tuple_terminal_inherited_constructor (13)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless match_arm_tuple_terminal_inherited_constructor --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_tuple_terminal_inherited_constructor (14)

Reason:
RIPR-SPEC-0122: state that no seam binds the finding and name the Next step instead of routing back to the gate (#9244)

Command:
`cargo xtask goldens bless match_arm_tuple_terminal_inherited_constructor --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_tuple_terminal_inherited_constructor (15)

Reason:
RIPR-SPEC-0122: define finding, unsuppressed, unknown, base-side and candidate line in the evidence legend (#9244)

Command:
`cargo xtask goldens bless match_arm_tuple_terminal_inherited_constructor --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
