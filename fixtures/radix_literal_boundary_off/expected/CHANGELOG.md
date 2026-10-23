# Golden Output Changes

## Pending — radix_literal_boundary_off (1)

Reason:
RIPR-SPEC-0001: new fixture pinning that a radix-prefixed literal input far from a 0xFF boundary stays weakly_exposed (#9164)

Command:
`cargo xtask goldens bless radix_literal_boundary_off --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — radix_literal_boundary_off (2)

Reason:
RIPR-SPEC-0001: add human-full golden for the radix off-boundary fixture (#9164)

Command:
`cargo xtask goldens bless radix_literal_boundary_off --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
