# Golden Output Changes

## Pending — rust_trait_impl_std_method_takes_receiver (1)

Reason:
RIPR-SPEC-0108: an owned receiver a standard trait method or an alias-impl macro method may take is name-only (#15956)

Command:
`cargo xtask goldens bless rust_trait_impl_std_method_takes_receiver --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
