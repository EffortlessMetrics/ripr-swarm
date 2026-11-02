# Golden Output Changes

## Pending — typescript_typeof_negation_compound_gap (1)

Reason:
RIPR-SPEC-0027: a negated typeof clause joined by && keeps the exact-kind rule and reads weak (#12281 review)

Command:
`cargo xtask goldens bless typescript_typeof_negation_compound_gap --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_typeof_negation_compound_gap (2)

Reason:
RIPR-SPEC-0027: re-bless after merging main (evidence legend wording discriminator->assertion)

Command:
`cargo xtask goldens bless typescript_typeof_negation_compound_gap --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_typeof_negation_compound_gap (3)

Reason:
RIPR-SPEC-0027: strong oracle with untied changed line no longer asks for another exact-value assertion (#12463)

Command:
`cargo xtask goldens bless typescript_typeof_negation_compound_gap --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
