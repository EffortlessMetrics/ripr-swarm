# Fixture: module_reach_include_spaced_shadow

Spec: RIPR-SPEC-0159

Owner: analysis-fixtures

Issue: #8999

## Given

The test crate includes a file declaring `mod qual_reach` through `include /* c */ !(..)`. The changed line is the private helper `is_bulk` in `src/inner.rs`.

## When

```bash
cargo xtask fixtures module_reach_include_spaced_shadow
```

## Then

Not related: an include-family macro in any spelling may rebind the library name.

## Must Not

- Promote the finding on a counterexample, or lose the relation on the controls.
