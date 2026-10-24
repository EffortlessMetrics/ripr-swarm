# Fixture: cross_crate_trait_impl_direct_owner_positive

Spec: RIPR-SPEC-0108

## Given

The positive control of `cross_crate_trait_impl_borrows_no_doctest`: the same
two-crate workspace (`Stream` declared in `rcore`, implemented by
`rutil::stream::take::Take`), with `Take::new` added and an integration test,
`util/tests/stream_owner.rs`, that calls `Take::size_hint` through the trait
and asserts the returned bound.

## When

```bash
cargo xtask fixtures cross_crate_trait_impl_direct_owner_positive
```

## Then

The direct owner call keeps its supported evidence: the finding is not
`no_static_path`, so the sibling-crate trait rule (#10061) refuses only
borrowed proximity credit, never a test that names the owner.

## Must Not

- Read `no_static_path` while a test calls `size_hint` on the owner.
- Claim runtime mutation adequacy; the `<` to `<=` edit returns the same bound
  at equality.
