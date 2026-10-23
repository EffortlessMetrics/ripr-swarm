# Golden Output Changes

## Pending — rust_ffi_attribute_literals (1)

Reason:
RIPR-SPEC-0062: raw doc strings and comments are not FFI attributes; preserve the real unsafe(no_mangle) after raw documentation. Three public check_workspace controls fail before and pass after the lexical split repair.

Command:
`cargo xtask goldens bless rust_ffi_attribute_literals --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_ffi_attribute_literals (2)

Reason:
RIPR-SPEC-0002: #7755 double-colon fix (language counts use =); #7756 labelled human lines carry no terminal period

Command:
`cargo xtask goldens bless rust_ffi_attribute_literals --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_ffi_attribute_literals (3)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless rust_ffi_attribute_literals --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_ffi_attribute_literals (4)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless rust_ffi_attribute_literals --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_ffi_attribute_literals (5)

Reason:
RIPR-SPEC-0122: no_static_path Weakness list no longer restates the Evidence block (#9033)

Command:
`cargo xtask goldens bless rust_ffi_attribute_literals --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_ffi_attribute_literals (6)

Reason:
RIPR-SPEC-0122: route-not-ready refusal names its missing evidence (#9604 batch C)

Command:
`cargo xtask goldens bless rust_ffi_attribute_literals --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_ffi_attribute_literals (7)

Reason:
RIPR-SPEC-0122: explain bounded Evidence stage and state meanings for #9373 item 1; retain full-human and JSON contracts

Command:
`cargo xtask goldens bless rust_ffi_attribute_literals --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_ffi_attribute_literals (8)

Reason:
RIPR-SPEC-0122: #9373 item 1 review repair; missing discriminators include absent assertions as well as inputs

Command:
`cargo xtask goldens bless rust_ffi_attribute_literals --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_ffi_attribute_literals (9)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — rust_ffi_attribute_literals (10)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless rust_ffi_attribute_literals --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
