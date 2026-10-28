# Golden Output Changes

## Pending — rust_qualified_helper_effect_reach (1)

Reason:
RIPR-SPEC-0159 qualified-hop reach must not borrow effect propagation; independent review and public API red control reproduced exposed before the refusal

Command:
`cargo xtask goldens bless rust_qualified_helper_effect_reach --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
