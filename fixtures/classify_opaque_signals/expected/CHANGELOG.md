# Golden Output Changes

## Pending — classify_opaque_signals (1)

Reason:
RIPR-SPEC-0009: pin pre-refactor opaque signal disclosures for #3023

Command:
`cargo xtask goldens bless classify_opaque_signals --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — classify_opaque_signals (2)

Reason:
RIPR-SPEC-0009: re-pin the opaque-signal disclosure baseline against current main; evidence_path became evidence_stages and the state wording moved, classification, exposed counts and stop reasons unchanged

Command:
`cargo xtask goldens bless classify_opaque_signals --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
