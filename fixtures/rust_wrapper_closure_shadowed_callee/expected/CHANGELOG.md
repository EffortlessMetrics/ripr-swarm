# Golden Output Changes

## Pending — rust_wrapper_closure_shadowed_callee (1)

Reason:
RIPR-SPEC-0186 #10284: a wrapper that shadows its callee with a closure promotes neither twin

Command:
`cargo xtask goldens bless rust_wrapper_closure_shadowed_callee --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
