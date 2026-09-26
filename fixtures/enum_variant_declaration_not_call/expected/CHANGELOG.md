# Golden Output Changes

## Pending — enum_variant_declaration_not_call (1)

Reason:
RIPR-SPEC-0046: pin tuple enum variants and tuple structs as static_unknown while constructor and function calls stay call_deletion (#3740)

Command:
`cargo xtask goldens bless enum_variant_declaration_not_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
