# Golden Output Changes

## Pending — macro_passthrough_sibling_item_macro_binding (1)

Reason:
RIPR-SPEC-0117: identity item macro with a sibling binding macro never promotes the wrapped test (#8999 item 5)

Command:
`cargo xtask goldens bless macro_passthrough_sibling_item_macro_binding --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — macro_passthrough_sibling_item_macro_binding (2)

Reason:
RIPR-SPEC-0122 RIPR-SPEC-0117: re-bless the sibling-binding fixture against the merged renderer legend; wrapped test stays no_static_path, not promoted

Command:
`cargo xtask goldens bless macro_passthrough_sibling_item_macro_binding --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
