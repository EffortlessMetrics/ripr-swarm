# Golden Output Changes

## Pending — rust_tolerance_assertion_macro_limitation (1)

Reason:
RIPR-SPEC-0120: tolerance assertion macros get a numeric next step in the named macro limitation (#6645)

Command:
`cargo xtask goldens bless rust_tolerance_assertion_macro_limitation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_tolerance_assertion_macro_limitation (2)

Reason:
RIPR-SPEC-0120: tolerance next step now words eq and ne macros by direction (#6645)

Command:
`cargo xtask goldens bless rust_tolerance_assertion_macro_limitation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_tolerance_assertion_macro_limitation (3)

Reason:
RIPR-SPEC-0423: additive evidence_stages rows and agent stages= line; human rendering unchanged

Command:
`cargo xtask goldens bless rust_tolerance_assertion_macro_limitation --reason "..."`

Updated:
- `expected/check.json`

## Pending — rust_tolerance_assertion_macro_limitation (4)

Reason:
RIPR-SPEC-0423: additive evidence_stages rows and agent stages= line; no test attribution; human rendering unchanged

Command:
`cargo xtask goldens bless rust_tolerance_assertion_macro_limitation --reason "..."`

Updated:
- `expected/check.json`

## Pending — rust_tolerance_assertion_macro_limitation (5)

Reason:
RIPR-SPEC-0117: #15237 the no_assertion row of a test calling an unclassified assertion-like macro reads assertion_admission_unproven and the finding names the test and macro; no oracle or class change

Command:
`cargo xtask goldens bless rust_tolerance_assertion_macro_limitation --reason "..."`

Updated:
- `expected/check.json`
