# Golden Output Changes

## Pending — radix_literal_boundary_match (1)

Reason:
RIPR-SPEC-0001: new control fixture pinning that a radix-prefixed literal input spelled as the 0xFF boundary stays exposed (#9164)

Command:
`cargo xtask goldens bless radix_literal_boundary_match --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — radix_literal_boundary_match (2)

Reason:
RIPR-SPEC-0001: add human-full golden for the radix matching-boundary control (#9164)

Command:
`cargo xtask goldens bless radix_literal_boundary_match --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — radix_literal_boundary_match (3)

Reason:
RIPR-SPEC-0001: human repair-route line gained missing-evidence reason on main; formatting only

Command:
`cargo xtask goldens bless radix_literal_boundary_match --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
