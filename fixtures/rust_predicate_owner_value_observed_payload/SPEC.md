# Fixture: rust_predicate_owner_value_observed_payload

Spec: RIPR-SPEC-0094

Owner: analysis-fixtures

Issue: #14346

## Given

A predicate that is the owner's returned tail changes from `qty > 10` to
`qty >= 10`. The one test calls the owner at the boundary input inside
`Some(..)` and compares the whole option with `Some(true)`, so the compared
value carries the call's result.

## When

`cargo xtask fixtures rust_predicate_owner_value_observed_payload` runs the public diff analyzer.

## Then

The predicate finding reads `exposed` with a strong exact-value oracle. This is
the control for the masked fixtures: the #14206 gate withholds only when the
owner call's value does not reach the compared operand.

```expected-classes
probe:src_lib.rs:predicate:63b83c61 -> exposed
probe:src_lib.rs:return_value:23a22a72 -> weakly_exposed
```

## Must Not

- Withhold the strong oracle from an assertion whose compared value carries the
  owner call's result as a variant payload.
- Claim a runtime mutation outcome; this is static evidence only.
