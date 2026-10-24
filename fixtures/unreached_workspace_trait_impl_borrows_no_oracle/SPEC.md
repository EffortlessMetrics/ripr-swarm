# Fixture: unreached_workspace_trait_impl_borrows_no_oracle

Spec: RIPR-SPEC-0108

## Given

`Inc::process` implements the workspace's own `Step` trait, and its error
message changes from `"ovf"` to `"overflow"`. No source calls `process`, and
the only test, `other_test`, pins the exact error of a different function,
`other`, in the same file.

## When

```bash
cargo xtask fixtures unreached_workspace_trait_impl_borrows_no_oracle
```

## Then

The finding is `no_static_path`: a workspace-declared trait's method runs only
where its name is spelled, so with nothing naming it there is no path, and the
same-file test is a suggested location, not a reach (#9169).

## Must Not

- Report `weakly_exposed` with "Strong oracle found: exact error variant
  assertion" borrowed from `other_test`.
- Treat the workspace trait impl like `Display::fmt`, which the language
  dispatches without naming it.
- Claim runtime mutation adequacy.
