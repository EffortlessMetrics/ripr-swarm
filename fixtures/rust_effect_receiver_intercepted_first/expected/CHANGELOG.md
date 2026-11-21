# Golden Output Changes

## Pending — rust_effect_receiver_intercepted_first (1)

Reason:
RIPR-SPEC-0108: refuse intercepted first observer; native correct and wrong payload controls both pass, preserving weak evidence

Command:
`cargo xtask goldens bless rust_effect_receiver_intercepted_first --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_effect_receiver_intercepted_first (2)

Reason:
RIPR-SPEC-0108: retain composed producer refusal of intercepted first observers with native correct/wrong controls; no exposure promotion

Command:
`cargo xtask goldens bless rust_effect_receiver_intercepted_first --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_effect_receiver_intercepted_first (3)

Reason:
RIPR-SPEC-0108 For rust_effect_receiver_intercepted_first, preserve structured evidence stages from RIPR-SPEC-0423.

Command:
`cargo xtask goldens bless rust_effect_receiver_intercepted_first --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_effect_receiver_intercepted_first (4)

Reason:
RIPR-SPEC-0108: The notifier.send call remains present on both sides. Describe the changed argument in the full human view rather than claiming removal (#15364).

Command:
`cargo xtask goldens bless rust_effect_receiver_intercepted_first --reason "..."`

Updated:
- `expected/human-full.txt`
