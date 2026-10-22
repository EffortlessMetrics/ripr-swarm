# Golden Output Changes

## Pending — typescript_enum_wrong_export (1)

Reason:
RIPR-SPEC-0243 rule 15 canonical enum identity; independently discriminated runtime and nonempty exact-oracle control

Command:
`cargo xtask goldens bless typescript_enum_wrong_export --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_enum_wrong_export (2)

Reason:
RIPR-SPEC-0243 rule 15 canonical enum identity; independently discriminated runtime and nonempty exact-oracle control

Command:
`cargo xtask goldens bless typescript_enum_wrong_export --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_enum_wrong_export (3)

Reason:
RIPR-SPEC-0243: #9056 now assigns a row-local related-test miss to TypeScript findings; this fixture (#9798) predates it, so the unchanged weakly_exposed finding gains miss=observation_unconfirmed (#9878)

Command:
`cargo xtask goldens bless typescript_enum_wrong_export --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
