# Golden Output Changes

## Pending — python_parametrize_tuple_unpack (1)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: a leading unpack of a tuple parametrize argname reads its rows, so off-boundary rows are weakly_exposed (#10832)

Command:
`cargo xtask goldens bless python_parametrize_tuple_unpack --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_parametrize_tuple_unpack (2)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless python_parametrize_tuple_unpack --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_parametrize_tuple_unpack (3)

Reason:
RIPR-SPEC-0122 plain wording: non-Rust terminal says spot list / missing check, Python preview next step says missing check (#11619)

Command:
`cargo xtask goldens bless python_parametrize_tuple_unpack --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_parametrize_tuple_unpack (3)

Reason:
RIPR-SPEC-0182: Python finding ids move to the versioned v1- namespace (#11368); classification and finding count unchanged; fixture diff and input made consistent so source context is verifiable

Command:
`cargo xtask goldens bless python_parametrize_tuple_unpack --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_parametrize_tuple_unpack (4)

Reason:
RIPR-SPEC-0118 / #10692: plain wording for seam and discriminator in default check and repair-start text; verdicts, identity and JSON fields unchanged

Command:
`cargo xtask goldens bless python_parametrize_tuple_unpack --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_parametrize_tuple_unpack (5)

Reason:
RIPR-SPEC-0122: explain missing input or assertion honestly

Command:
`cargo xtask goldens bless python_parametrize_tuple_unpack --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
