# Fixture: rust_computed_return_tail_crate_rebound

Spec: RIPR-SPEC-0443

## Given

The same change as `rust_computed_return_tail_reached` (`base * 3` to
`base * 2` under `if express`). The integration test writes
`computed_return_tail_crate_rebound_fixture::shipping_cost(100, 10, true)
== 800`, but its file declares a local module with the library's name whose
`shipping_cost` always returns `800`. The path names that stub, and the test
never calls the owner.

## When

```bash
cargo xtask fixtures rust_computed_return_tail_crate_rebound
```

or:

```bash
ripr check --root fixtures/rust_computed_return_tail_crate_rebound/input --diff fixtures/rust_computed_return_tail_crate_rebound/diff.patch --mode fast
```

## Then

The changed return is not promoted to `exposed` (#13037): the library name
is rebound in the test file, so the own-crate path is not established and
the crate-qualified pin is not read as an owner call.

```expected-classes
probe:src_lib.rs:return_value:f222f9b2 -> reachable_unrevealed
```

## Must Not

- Credit a crate-qualified pin whose crate name the test file rebinds.
