# Fixture: rust_stub_unrelated_shadow_expected_value

Spec: RIPR-SPEC-0231

Owner: analysis-fixtures

Issue: #14868

## Given

The boundary test compares the changed price with an unfilled `expected`
local. An unrelated closure parameter and a completed match arm both use
the same name.

## When

`cargo xtask fixtures rust_stub_unrelated_shadow_expected_value` analyzes the predicate change.

## Then

The public check reports one `weakly_exposed` finding and explains why
the placeholder assertion earns no credit. Removing the unrelated scopes
leaves the same finding.

```expected-classes
probe:src_lib.rs:predicate:c80557eb -> weakly_exposed
```

## Must Not

Crediting either unrelated binding as the expected result would make the
boundary mutation appear covered. The fixture must keep the finding weak.
