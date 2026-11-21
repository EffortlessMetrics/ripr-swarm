# Golden Output Changes

## Pending — rust_trait_inherent_twin (1)

Reason:
RIPR-SPEC-0108: a call that dispatches to an override or an inherent twin is name-only, not direct_owner_call (#15273)

Command:
`cargo xtask goldens bless rust_trait_inherent_twin --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
