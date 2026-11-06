# Golden Output Changes

## Pending — error_path_removed_variant_diagnostic_not_credited (1)

Reason:
RIPR-SPEC-0106: #12110 review fix; a removed variant named only in a diagnostic argument is not a pin

Command:
`cargo xtask goldens bless error_path_removed_variant_diagnostic_not_credited --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — error_path_removed_variant_diagnostic_not_credited (2)

Reason:
RIPR-SPEC-0422: additive evidence_stages rows and agent stages= line; human rendering unchanged

Command:
`cargo xtask goldens bless error_path_removed_variant_diagnostic_not_credited --reason "..."`

Updated:
- `expected/check.json`
