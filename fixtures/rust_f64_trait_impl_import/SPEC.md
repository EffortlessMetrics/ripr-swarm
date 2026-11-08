# Fixture: rust_f64_trait_impl_import

Spec: RIPR-SPEC-0108
Related spec: RIPR-SPEC-0197
Issue: #7117 (control for #7053: `use super::Render` keeps direct dispatch)

## Given

Same production `impl Render for f64` as the shadow fixture, but the test module imports the production trait. The test calls `(-0.0f64).render()` with the same literal pin.

## When

```bash
cargo xtask fixtures rust_f64_trait_impl_import
```

## Then

Exactly one `return_value` finding reads `exposed` with `direct_owner_call`. Wrong-production mutants fail the test.

## Must Not

- Credit the test when it calls a shadowed trait impl.
- Accept a missing finding or test as promotion pass.
