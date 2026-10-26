# Golden Output Changes

## Pending — rust_lossy_post_operation_pin (1)

Reason:
RIPR-SPEC-0197: a pin the lossy last operation hides the change from reads weakly_exposed (#10737)

Command:
`cargo xtask goldens bless rust_lossy_post_operation_pin --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_lossy_post_operation_pin (2)

Reason:
RIPR-SPEC-0197: human-full golden for the lossy last operation pin (#10737)

Command:
`cargo xtask goldens bless rust_lossy_post_operation_pin --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
