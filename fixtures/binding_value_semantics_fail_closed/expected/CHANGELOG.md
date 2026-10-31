# Golden Output Changes

## Pending — binding_value_semantics_fail_closed (1)

Reason:
Native Rust and public API red/green controls reject signed host-index folding and skipped eager defaults; preserve unknown predicate boundaries under RIPR-SPEC-0158.

Command:
`cargo xtask goldens bless binding_value_semantics_fail_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — binding_value_semantics_fail_closed (2)

Reason:
RIPR-SPEC-0158: retain full human output alongside JSON so the independent honesty corpus checks both projections of the two unknown predicate boundaries.

Command:
`cargo xtask goldens bless binding_value_semantics_fail_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — binding_value_semantics_fail_closed (3)

Reason:
RIPR-SPEC-0158 and RIPR-SPEC-0319: compose with merged #9449 guidance by retaining the two current-main stop-reason Next lines; JSON, classifications, stages and default human output remain unchanged.

Command:
`cargo xtask goldens bless binding_value_semantics_fail_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — binding_value_semantics_fail_closed (4)

Reason:
RIPR-SPEC-0158 and RIPR-SPEC-0319: retain the complete missing-discriminator prose under accepted #10258 compact-human wrapping. Exact observed merge8272 reproduces only this owned human.txt drift; JSON and human-full are byte-identical. Preserve the unknown predicates and existing returned-comparison residual.

Command:
`cargo xtask goldens bless binding_value_semantics_fail_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — binding_value_semantics_fail_closed (5)

Reason:
RIPR-SPEC-0158: add the native-confirmed shadowed usize=u8 direct-expression counterexample. Independent assertions require exactly three unknown predicates with no equality or missing input; preserve the separate inherited returned-comparison fallback, not a correctness claim for it.

Command:
`cargo xtask goldens bless binding_value_semantics_fail_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — binding_value_semantics_fail_closed (6)

Reason:
RIPR-SPEC-0158: retain accepted main terminology legend (#10426) and repair recovery guidance (#10382) in this owned compact-human fixture; JSON, human-full and predicate unknown states remain unchanged

Command:
`cargo xtask goldens bless binding_value_semantics_fail_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — binding_value_semantics_fail_closed (7)

Reason:
RIPR-SPEC-0158: compose with current main plain-language summary and related-test guidance (#11104); classifications, stages and unknown predicates unchanged, only summary/recommended_next_step wording from main

Command:
`cargo xtask goldens bless binding_value_semantics_fail_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — binding_value_semantics_fail_closed (8)

Reason:
RIPR-SPEC-0122: #9373 hidden rows name the unknown stage and gloss listed classes

Command:
`cargo xtask goldens bless binding_value_semantics_fail_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — binding_value_semantics_fail_closed (9)

Reason:
RIPR-SPEC-0197: #10046 returned comparisons share the predicate lens's unresolved-input reading, so a returned comparison whose boundary input is a computed call result or an unresolved owner call is infection_unknown instead of the inherited exposed fallback; no gap or equality is invented

Command:
`cargo xtask goldens bless binding_value_semantics_fail_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
