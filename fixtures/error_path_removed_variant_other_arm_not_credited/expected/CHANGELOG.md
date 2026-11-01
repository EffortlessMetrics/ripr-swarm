# Golden Output Changes

## Pending — error_path_removed_variant_other_arm_not_credited (1)

Reason:
RIPR-SPEC-0106: new fixture for #12110; a pin of the removed variant through an input that selects another arm does not discriminate the change

Command:
`cargo xtask goldens bless error_path_removed_variant_other_arm_not_credited --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
