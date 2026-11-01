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
