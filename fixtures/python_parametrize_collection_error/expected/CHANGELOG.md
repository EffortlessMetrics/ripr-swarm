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
RIPR-SPEC-0122: #11619 plain wording (spot) for the check screen's repair-start refusals

Command:
`cargo xtask goldens bless python_parametrize_collection_error --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
