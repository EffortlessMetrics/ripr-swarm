# Golden Output Changes

## Pending — macro_passthrough_sibling_item_macro_binding (1)

Reason:
RIPR-SPEC-0117: identity item macro with a sibling binding macro never promotes the wrapped test (#8999 item 5)

Command:
`cargo xtask goldens bless macro_passthrough_sibling_item_macro_binding --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
