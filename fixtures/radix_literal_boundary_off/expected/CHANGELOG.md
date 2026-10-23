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

## Pending — radix_literal_boundary_off (3)

Reason:
RIPR-SPEC-0001: human repair-route line gained missing-evidence reason on main; formatting only

Command:
`cargo xtask goldens bless radix_literal_boundary_off --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — radix_literal_boundary_off (4)

Reason:
RIPR-SPEC-0122: #9373 item 1 selected Evidence legend coverage for the newly accepted radix-boundary fixture, preserving its current facts and all five stage values.

Operation:
Manual edit: inserted the existing five-line bounded Evidence legend immediately after the selected Evidence line.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`
