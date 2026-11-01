# Golden Output Changes

## Pending — error_path_removed_variant_shadowed_not_credited (1)

Reason:
RIPR-SPEC-0106: #12110 review fixes; pinned fixture no longer lists the already-credited discriminator as missing; diagnostic and shadowed negatives added

Command:
`cargo xtask goldens bless error_path_removed_variant_shadowed_not_credited --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
