# Golden Output Changes

## Pending — rust_trait_impl_constructed_receiver (1)

Reason:
RIPR-SPEC-0108: a trait impl receiver typed by an indexed constructor is direct; an owned receiver taken by an inherent by-value twin is name-only (#15956, #15428)

Command:
`cargo xtask goldens bless rust_trait_impl_constructed_receiver --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_trait_impl_constructed_receiver (2)

Reason:
RIPR-SPEC-0108: a trait impl receiver typed by an indexed constructor is direct; an owned receiver taken by an inherent by-value twin is name-only (#15956, #15428)

Command:
`cargo xtask goldens bless rust_trait_impl_constructed_receiver --reason "..."`

Updated:
- `expected/human-full.txt`

## Pending — rust_trait_impl_constructed_receiver (3)

Reason:
RIPR-SPEC-0108: on current main the direct call with an exact-value assertion reads exposed; the relation is unchanged

Command:
`cargo xtask goldens bless rust_trait_impl_constructed_receiver --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
