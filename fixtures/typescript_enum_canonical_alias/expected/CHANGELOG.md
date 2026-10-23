# Golden Output Changes

## Pending — typescript_enum_canonical_alias (1)

Reason:
RIPR-SPEC-0243 rule 15 canonical enum identity; independently discriminated runtime and nonempty exact-oracle control

Command:
`cargo xtask goldens bless typescript_enum_canonical_alias --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_enum_canonical_alias (2)

Reason:
RIPR-SPEC-0243 rule 15 canonical enum identity; independently discriminated runtime and nonempty exact-oracle control

Command:
`cargo xtask goldens bless typescript_enum_canonical_alias --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_enum_canonical_alias (3)

Reason:
RIPR-SPEC-0122: #9373 item 1 bounded Evidence legend on new current-main fixture; preserve current JSON, human-full and discriminator facts

Command:
`cargo xtask goldens bless typescript_enum_canonical_alias --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_enum_canonical_alias (4)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`
