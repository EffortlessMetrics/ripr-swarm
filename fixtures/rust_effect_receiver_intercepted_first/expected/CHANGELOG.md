# Golden Output Changes

## Pending — rust_effect_receiver_intercepted_first (1)

Reason:
RIPR-SPEC-0108: refuse intercepted first observer; native correct and wrong payload controls both pass, preserving weak evidence

Command:
`cargo xtask goldens bless rust_effect_receiver_intercepted_first --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
