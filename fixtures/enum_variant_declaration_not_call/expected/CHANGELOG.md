# Golden Output Changes

## Pending — enum_variant_declaration_not_call (1)

Reason:
RIPR-SPEC-0046: pin tuple enum variants and tuple structs as static_unknown while constructor and function calls stay call_deletion (#3740)

Command:
`cargo xtask goldens bless enum_variant_declaration_not_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — enum_variant_declaration_not_call (2)

Reason:
RIPR-SPEC-0122: bounded human surfaces disclose their windows - related-test and observed-value caps, digest missing-discriminator and related-test totals, Hidden block names omitted findings by file:line (class) with the all-base-side distinction (#4320); RIPR-SPEC-0152: all-base-side runs name base-side evidence instead of a lower-priority framing

Command:
`cargo xtask goldens bless enum_variant_declaration_not_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
