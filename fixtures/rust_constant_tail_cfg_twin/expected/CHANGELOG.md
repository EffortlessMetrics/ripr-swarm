# Golden Output Changes

## Pending — rust_constant_tail_cfg_twin (1)

Reason:
RIPR-SPEC-0197: a cfg-gated owner's changed integer constant tail reads propagation_unknown (#15104)

Command:
`cargo xtask goldens bless rust_constant_tail_cfg_twin --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
