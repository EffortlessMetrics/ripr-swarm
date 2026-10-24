# Fixture: same_crate_trait_impl_same_leaf_doctest_control

Spec: RIPR-SPEC-0108

## Given

The control of `cross_crate_trait_impl_borrows_no_doctest`: the same two-file
layout with `Stream` declared in the impl's own crate (`use crate::Stream;`).

## When

```bash
cargo xtask fixtures same_crate_trait_impl_same_leaf_doctest_control
```

## Then

The finding is `no_static_path`, the same class as the cross-crate fixture, so
the sibling-crate trait is judged exactly like the same-crate one (#9169,
#10061).

## Must Not

- Differ in class from the cross-crate fixture.
- Claim runtime mutation adequacy.
