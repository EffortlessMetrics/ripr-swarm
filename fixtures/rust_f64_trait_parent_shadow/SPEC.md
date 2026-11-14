# Fixture: rust_f64_trait_parent_shadow

Spec: RIPR-SPEC-0108
Related spec: RIPR-SPEC-0197
Issue: #7117 (#7087 out-of-line parent declares `trait Render`)

## Given

Production `impl Render for f64` and `#[cfg(test)] mod helpers;`. The out-of-line `helpers.rs` parent redeclares `trait Render` before `mod render_tests;`. The nested test calls `(-0.0f64).render()`.

The assertion executes the parent-local trait, not the production impl.

## When

```bash
cargo xtask fixtures rust_f64_trait_parent_shadow
```

## Then

Exactly one `return_value` finding must not promote (`reachable_unrevealed`, `weak_token_substring`). Wrong-production mutants keep the test green.

## Must Not

- Infer production dispatch from the method name or assertion tokens alone.
- Accept a missing finding or test as a non-promotion pass.
