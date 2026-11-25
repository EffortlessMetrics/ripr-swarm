# Fixture: rust_generic_trait_sibling_impl

Spec: RIPR-SPEC-0108
Issue: #14074 (a call typed as a sibling generic impl's argument)

## Given

Production declares `struct Cents` with two impls of the same generic trait on it: `impl From<u32> for Cents` and `impl From<&str> for Cents`. The diff changes the body of the `&str` impl. The only test calls `Cents::from(2u32)` and pins the result with `assert_eq!`.

The suffixed literal selects the `u32` impl; the assertion never executes the changed `&str` impl.

## When

```bash
cargo xtask fixtures rust_generic_trait_sibling_impl
```

## Then

Exactly one `return_value` finding must not promote to `exposed` (`reachable_unrevealed` with `weak_token_substring` on the related test). Wrong-production mutants of the `&str` impl keep the test green.

## Must Not

- Credit a direct owner call from the shared method name and self type alone when the argument type names the sibling impl.
- Accept a missing finding or test as a non-promotion pass.
- Refuse a call whose argument type is not read from the call site; that stays outside this fixture.
