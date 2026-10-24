# Golden Output Changes

## Pending — same_crate_trait_impl_same_leaf_doctest_control (1)

Reason:
RIPR-SPEC-0108: new control fixture, trait in the impl's own crate stays no_static_path (#10061)

Command:
`cargo xtask goldens bless same_crate_trait_impl_same_leaf_doctest_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
