# Golden Output Changes

## Pending — rust_test_under_false_cfg_not_grip (1)

Reason:
RIPR-SPEC-0153: new fixture; a #[test] under a never-true cfg is not discovered and cannot grip the change (#6293)

Command:
`cargo xtask goldens bless rust_test_under_false_cfg_not_grip --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_test_under_false_cfg_not_grip (2)

Reason:
RIPR-SPEC-0153: main now renders workspace-relative finding paths (./src/lib.rs) and adds canonical-gap fields; static_unknown verdict and reach unchanged (#6293)

Command:
`cargo xtask goldens bless rust_test_under_false_cfg_not_grip --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
