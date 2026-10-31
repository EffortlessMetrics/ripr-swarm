# Golden Output Changes

## Pending — binding_value_counterpart_width (1)

Reason:
RIPR-SPEC-0158: compiler-backed u8/i8 counterpart constraints refuse fabricated equality from bool-selected arithmetic; preserve unknown predicates and record the inherited return fallback separately

Command:
`cargo xtask goldens bless binding_value_counterpart_width --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — binding_value_counterpart_width (2)

Reason:
RIPR-SPEC-0158 and RIPR-SPEC-0108: retain full human projection alongside JSON/compact output for the compiler-backed narrow counterpart non-promotion case; predicates stay unknown and separate return findings stay propagation_unknown

Command:
`cargo xtask goldens bless binding_value_counterpart_width --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — binding_value_counterpart_width (3)

Reason:
RIPR-SPEC-0158: compose with current main plain-language summary and related-test guidance (#11104); classifications, stages and unknown predicates unchanged, only summary/recommended_next_step wording from main

Command:
`cargo xtask goldens bless binding_value_counterpart_width --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — binding_value_counterpart_width (4)

Reason:
RIPR-SPEC-0122: #9373 hidden rows name the unknown stage and gloss listed classes

Command:
`cargo xtask goldens bless binding_value_counterpart_width --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
