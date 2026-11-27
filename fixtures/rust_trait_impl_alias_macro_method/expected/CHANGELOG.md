# Golden Output Changes

## Pending — rust_trait_impl_alias_macro_method (1)

Reason:
RIPR-SPEC-0108: an owned receiver a standard trait method or an alias-impl macro method may take is name-only (#15956)

Command:
`cargo xtask goldens bless rust_trait_impl_alias_macro_method --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
