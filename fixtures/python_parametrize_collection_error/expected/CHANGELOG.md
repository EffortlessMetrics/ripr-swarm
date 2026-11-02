# Golden Output Changes

## Pending — python_parametrize_collection_error (1)

Reason:
RIPR-SPEC-0238 and RIPR-SPEC-0108: a parametrize mark that fails pytest collection lends the test no credit, so a refused sibling mark leaves the boundary decorator_indirection (#10867)

Command:
`cargo xtask goldens bless python_parametrize_collection_error --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_parametrize_collection_error (2)

Reason:
RIPR-SPEC-0122: no-gap-ID refusal wording re-blessed now that the goldens gate compares it verbatim (#12003)

Command:
`cargo xtask goldens bless python_parametrize_collection_error --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_parametrize_collection_error (3)

Reason:
RIPR-SPEC-0182: Python finding ids move to the versioned v1- namespace (#11368); classification and finding count unchanged; fixture diff and input made consistent so source context is verifiable

Command:
`cargo xtask goldens bless python_parametrize_collection_error --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_parametrize_collection_error (4)

Reason:
RIPR-SPEC-0118 / #10692: plain wording for seam and discriminator in default check and repair-start text; verdicts, identity and JSON fields unchanged

Command:
`cargo xtask goldens bless python_parametrize_collection_error --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_parametrize_collection_error (5)

Reason:
RIPR-SPEC-0122: explain missing input or assertion honestly

Command:
`cargo xtask goldens bless python_parametrize_collection_error --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
