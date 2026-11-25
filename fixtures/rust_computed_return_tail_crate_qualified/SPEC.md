# Fixture: rust_computed_return_tail_crate_qualified

Spec: RIPR-SPEC-0443

## Given

The same change as `rust_computed_return_tail_reached` (`base * 3` to
`base * 2` under `if express`). The only test is an integration test with no
`use`: it pins the owner through the package's own library path,
`computed_return_tail_crate_qualified_fixture::shipping_cost(100, 10, true)
== 800`. Nothing in the test file declares, imports, renames or globs the
library name, so the path names the owner.

## When

```bash
cargo xtask fixtures rust_computed_return_tail_crate_qualified
```

or:

```bash
ripr check --root fixtures/rust_computed_return_tail_crate_qualified/input --diff fixtures/rust_computed_return_tail_crate_qualified/diff.patch --mode fast
```

## Then

The changed return reads `exposed` (#13037), the same as the bare imported
spelling: the own-crate path is read as the bare owner call, and that call
statically reaches the changed branch.

```expected-classes
probe:src_lib.rs:return_value:f222f9b2 -> exposed
```

## Must Not

- Read the change as `weakly_exposed` only because the test spells the
  owner through its crate path.
