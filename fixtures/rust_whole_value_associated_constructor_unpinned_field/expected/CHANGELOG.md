# Golden Output Changes

## Pending — rust_whole_value_associated_constructor_unpinned_field (1)

Reason:
RIPR-SPEC-0225: associated constructor literal neither version computes stays weakly exposed (#15345)

Command:
`cargo xtask goldens bless rust_whole_value_associated_constructor_unpinned_field --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_whole_value_associated_constructor_unpinned_field (2)

Reason:
RIPR-SPEC-0225: the exact constructor field stays weak because its strongest test reaches the owner without a verified field sink; retries 7 fails in both compiled source versions.

Command:
`cargo xtask goldens bless rust_whole_value_associated_constructor_unpinned_field --reason "..."`

Updated:
- `expected/check.json`
- `expected/human-full.txt`
