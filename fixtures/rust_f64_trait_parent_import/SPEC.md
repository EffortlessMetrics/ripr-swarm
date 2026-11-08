# Fixture: rust_f64_trait_parent_import

Spec: RIPR-SPEC-0108
Related spec: RIPR-SPEC-0197
Issue: #7117 (control for #7087: parent imports production `Render`)

## Given

Same out-of-line layout as the parent shadow fixture, but `helpers.rs` imports `crate::Render` and does not redeclare the trait.

## When

```bash
cargo xtask fixtures rust_f64_trait_parent_import
```

## Then

Exactly one `return_value` finding reads `exposed` with `direct_owner_call`. Wrong-production mutants fail the test.

## Must Not

- Credit the test when a parent module shadows the production trait.
- Accept a missing finding or test as promotion pass.
