# Golden Output Changes

## Pending — compile_error_build_time (1)

Reason:
RIPR-SPEC-0411: #15933 a changed compile_error! line, one-line or the message line of a split invocation, is one static_unknown finding with compile_time_invariant

Command:
`cargo xtask goldens bless compile_error_build_time --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — compile_error_build_time (2)

Reason:
RIPR-SPEC-0122: #15336 unconfirmed-reach note severity, possible-gap State line, legend and Terms glosses

Command:
`cargo xtask goldens bless compile_error_build_time --reason "..."`

Updated:
- `expected/human.txt`

## Pending — compile_error_build_time (3)

Reason:
RIPR-SPEC-0122: #15336 human-full Terms line stops claiming test reach for weakly_exposed and reachable_unrevealed

Command:
`cargo xtask goldens bless compile_error_build_time --reason "..."`

Updated:
- `expected/human-full.txt`
