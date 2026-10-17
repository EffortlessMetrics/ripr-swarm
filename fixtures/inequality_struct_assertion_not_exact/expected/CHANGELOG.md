# Golden Output Changes

## Pending — inequality_struct_assertion_not_exact (1)

Reason:
RIPR-SPEC-0231: assert_ne! against a struct literal stays whole_object_equality/weak under every probe family

Command:
`cargo xtask goldens bless inequality_struct_assertion_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — inequality_struct_assertion_not_exact (2)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless inequality_struct_assertion_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — inequality_struct_assertion_not_exact (3)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless inequality_struct_assertion_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — inequality_struct_assertion_not_exact (4)

Reason:
RIPR-SPEC-0122: re-bless for #8412 stub route/refusal now printed for every probe family (call_deletion, field_construction, side_effect); drift is solely the inserted stub-route line, no classification or count change

Command:
`cargo xtask goldens bless inequality_struct_assertion_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
