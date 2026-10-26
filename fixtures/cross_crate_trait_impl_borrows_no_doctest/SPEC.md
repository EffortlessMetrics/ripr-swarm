# Fixture: cross_crate_trait_impl_borrows_no_doctest

Spec: RIPR-SPEC-0108

## Given

`rutil::stream::take::Take` implements `Stream`, a trait declared in the sibling
workspace crate `rcore` and imported with `use rcore::Stream;`. Its
`size_hint` guard changes from `<` to `<=`. No source calls `size_hint`. The
only related test material is the doctest on `rutil::io::take::Take::limit`,
which shares only the leaf file name `take.rs` and observes a different type.

## When

```bash
cargo xtask fixtures cross_crate_trait_impl_borrows_no_doctest
```

## Then

The finding is `no_static_path`: a trait declared in any workspace member is
called only where its method name is spelled, so with nothing naming
`size_hint` there is no path, and the same-leaf-name doctest is a suggested
location, not a reach (#10061, the cross-crate case of #9169).

## Must Not

- Report `weakly_exposed` or `reachable_unrevealed` with an oracle borrowed from
  the `io/take.rs` doctest.
- Treat an import from a workspace member crate like an import of a foreign
  trait such as `std::ops::Add`.
- Claim runtime mutation adequacy.
