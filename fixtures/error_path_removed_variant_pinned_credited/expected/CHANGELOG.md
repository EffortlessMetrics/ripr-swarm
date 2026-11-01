# Golden Output Changes

## Pending — error_path_removed_variant_pinned_credited (1)

Reason:
RIPR-SPEC-0106: new fixture for #12110; an exact pin of the variant a changed error arm removed, on an owner call selecting that arm, discriminates the change

Command:
`cargo xtask goldens bless error_path_removed_variant_pinned_credited --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
