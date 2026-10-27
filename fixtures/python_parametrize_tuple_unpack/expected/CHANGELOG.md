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
