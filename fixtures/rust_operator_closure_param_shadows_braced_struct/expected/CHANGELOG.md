# Golden Output Changes

## Pending — rust_operator_closure_param_shadows_braced_struct (1)

Reason:
RIPR-SPEC-0001: a pattern binder named like a braced struct shadows its call spelling (#15801)

Command:
`cargo xtask goldens bless rust_operator_closure_param_shadows_braced_struct --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
