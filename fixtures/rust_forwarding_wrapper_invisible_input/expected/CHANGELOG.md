# Golden Output Changes

## Pending — rust_forwarding_wrapper_invisible_input (1)

Reason:
RIPR-SPEC-0197: a forwarding wrapper pinned where the private helper change is invisible is not exposed (#15034)

Command:
`cargo xtask goldens bless rust_forwarding_wrapper_invisible_input --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
