# Fixture: rust_f64_trait_impl_shadow

Spec: RIPR-SPEC-0108
Related spec: RIPR-SPEC-0197
Issue: #7117 (#7053 inline trait shadow with `impl Render for f64`)

## Given

Production declares `trait Render` and `impl Render for f64`. A same-file `#[cfg(test)] mod tests` redeclares the trait with its own `f64` impl. The test calls `(-0.0f64).render()` and pins the empty `String`.

The assertion executes the test-local trait; it never observes the changed production method.

## When

```bash
cargo xtask fixtures rust_f64_trait_impl_shadow
```

## Then

Exactly one `return_value` finding must not promote to `exposed` (`reachable_unrevealed` with `weak_token_substring` on the related test). Wrong-production mutants keep the test green.

## Must Not

- Infer production dispatch from the method name or assertion tokens alone.
- Accept a missing finding or test as a non-promotion pass.
- Infer runtime behavior from an uncompiled, manufactured `RustIndex`.
