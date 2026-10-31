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
#10697: plain wording for discriminator and seam in default human output; mirror the shared evidence/legend/no-seam rewording and reword TS preview discriminator labels to missing-check wording (safe, observed-advisory and exact-error-variant)

Command:
`cargo xtask goldens bless python_parametrize_tuple_unpack --reason "..."`

Updated:
- `expected/human.txt`
