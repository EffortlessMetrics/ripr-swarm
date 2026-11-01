# Golden Output Changes

## Pending — error_path_removed_variant_remapped_not_credited (1)

Reason:
RIPR-SPEC-0106: new fixture for #12110; a pin of the removed variant is not credited when the match result is remapped after the match

Command:
`cargo xtask goldens bless error_path_removed_variant_remapped_not_credited --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
