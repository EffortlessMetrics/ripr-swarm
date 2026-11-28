# Fixture: rust_generic_trait_sibling_impl_inferred_literal

Spec: RIPR-SPEC-0108
Issue: #14074 (an unsuffixed literal whose kind rules out the changed impl)

## Given

Production declares `struct Cents` with `impl From<u32> for Cents` and `impl From<&str> for Cents`. The diff changes the body of the `&str` impl. The only test calls `Cents::from(2)` and pins the result with `assert_eq!`.

The literal is unsuffixed, so its exact type comes from inference, but an integer literal is never `&str`: the call selects the `u32` impl and never executes the changed one.

## When

```bash
cargo xtask fixtures rust_generic_trait_sibling_impl_inferred_literal
```

## Then

Exactly one `return_value` finding must not promote to `exposed` (`reachable_unrevealed` with `weak_token_substring` on the related test). Wrong-production mutants of the `&str` impl keep the test green.

## Must Not

- Credit a direct owner call because the literal's exact type is left to inference.
- Accept a missing finding or test as a non-promotion pass.
- Rule out an owner whose argument is not a primitive; an alias may take the literal.
