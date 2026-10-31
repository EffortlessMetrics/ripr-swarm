# Golden Output Changes

## Pending — rust_binary_alias_foreign_dependency (1)

Reason:
RIPR-SPEC-0153: registry namesake has no bound test wrapper

Command:
`cargo xtask goldens bless rust_binary_alias_foreign_dependency --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_binary_alias_foreign_dependency (2)

Reason:
RIPR-SPEC-0153: include the full human evidence projection

Command:
`cargo xtask goldens bless rust_binary_alias_foreign_dependency --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_binary_alias_foreign_dependency (3)

Reason:
#10697: plain wording for discriminator and seam in default human output; mirror the shared evidence/legend/no-seam rewording and reword TS preview discriminator labels to missing-check wording (safe, observed-advisory and exact-error-variant)

Command:
`cargo xtask goldens bless rust_binary_alias_foreign_dependency --reason "..."`

Updated:
- `expected/human.txt`
