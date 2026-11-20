# Fixture: rust_generic_trait_sibling_impl_named_operator

Spec: RIPR-SPEC-0108
Issue: #14074 (an operator trait method called by name through a sibling impl)

## Given

Production declares `struct Meters` with `impl Add<u32> for Meters` and `impl Add<Meters> for Meters`. The diff changes the body of the `Add<Meters>` impl. The only test calls `Meters(1).add(2u32)` by name and pins the result with `assert_eq!`; it has no `+` and no `sum`.

The argument is a `u32`, so the call selects the `Add<u32>` impl and never executes the changed one. With no operator token in the test, nothing else in it can run `Add<Meters>` unnamed.

## When

```bash
cargo xtask fixtures rust_generic_trait_sibling_impl_named_operator
```

## Then

Exactly one `call_deletion` finding must not promote to `exposed` (`reachable_unrevealed` with `weak_token_substring` on the related test). Wrong-production mutants of the `Add<Meters>` impl keep the test green.

## Must Not

- Credit a direct owner call because the owner is an operator method.
- Accept a missing finding or test as a non-promotion pass.
- Refuse the direct call when the test also uses the operator token (`+`), or `sum` for `add`, which may run the owner unnamed.
