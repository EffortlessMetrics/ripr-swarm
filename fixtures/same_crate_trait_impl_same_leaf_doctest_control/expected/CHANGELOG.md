# Golden Output Changes

## Pending — same_crate_trait_impl_same_leaf_doctest_control (1)

Reason:
RIPR-SPEC-0108: new control fixture, trait in the impl's own crate stays no_static_path (#10061)

Command:
`cargo xtask goldens bless same_crate_trait_impl_same_leaf_doctest_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — same_crate_trait_impl_same_leaf_doctest_control (2)

Reason:
RIPR-SPEC-0108: miss wording from main's #5493 language-neutral wording after merging origin/main; class no_static_path unchanged (#10061)

Command:
`cargo xtask goldens bless same_crate_trait_impl_same_leaf_doctest_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — same_crate_trait_impl_same_leaf_doctest_control (3)

Reason:
RIPR-SPEC-0118 / docs/OUTPUT_SCHEMA.md: #10192 displays hidden finding kind/reason, exact ambiguous seam choices, and evidence-specific unresolved-edge guidance; verdict, stage, identity and oracle fields stay unchanged (independently compared against actual JSON).

Command:
`cargo xtask goldens bless same_crate_trait_impl_same_leaf_doctest_control --reason "..."`

Updated:
- `expected/human.txt`
