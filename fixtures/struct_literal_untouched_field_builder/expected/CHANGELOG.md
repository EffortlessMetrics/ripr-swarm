# Golden Output Changes

## Pending — struct_literal_untouched_field_builder (1)

Reason:
RIPR-SPEC-0040: #10916 untouched-field struct literal charter fixture; initial golden pins weakly_exposed for the partly changed literal

Command:
`cargo xtask goldens bless struct_literal_untouched_field_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — struct_literal_untouched_field_builder (2)

Reason:
RIPR-SPEC-0288: check lists only --seam-id stub choices that agent stub accepts (#10192)

Command:
`cargo xtask goldens bless struct_literal_untouched_field_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
