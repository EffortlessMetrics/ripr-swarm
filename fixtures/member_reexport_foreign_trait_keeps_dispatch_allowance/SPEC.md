# Fixture: member_reexport_foreign_trait_keeps_dispatch_allowance

Spec: RIPR-SPEC-0108

## Given

The foreign-trait control of `cross_crate_trait_impl_borrows_no_doctest`:
`rutil::stream::take::Take` implements `Display` imported as
`use rmid::Display;`, where the workspace member `rmid` only re-exports
`std::fmt::Display`. No workspace member declares a trait named `Display`.
The `fmt` guard changes from `<` to `<=` and no test names `fmt`.

## When

```bash
cargo xtask fixtures member_reexport_foreign_trait_keeps_dispatch_allowance
```

## Then

A genuine foreign trait keeps the dispatch allowance of #9342: `Display::fmt`
runs without spelling `fmt`, so the same-leaf-name doctest stays a possible
reach and the finding is not `no_static_path` (the sibling-crate trait rule of
#10061 does not apply, because no member declares the trait).

## Must Not

- Treat a member's re-export of a foreign trait as a workspace-declared trait.
- Claim runtime mutation adequacy.
