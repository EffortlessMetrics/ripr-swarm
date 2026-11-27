# Fixture: rust_predicate_owner_value_masked_is_some

Spec: RIPR-SPEC-0094

Owner: analysis-fixtures

Issue: #14346

## Given

A predicate that is the owner's returned tail changes from `qty > 10` to
`qty >= 10`. The one test calls the owner at the boundary input inside
`Some(..)` and compares `.is_some()`, which is `true` for either version.

## When

`cargo xtask fixtures rust_predicate_owner_value_masked_is_some` runs the public diff analyzer.

## Then

The predicate finding reads `weakly_exposed`: the related test is credited no
stronger than weak, with the miss `observation_unconfirmed`, because the
compared value does not carry the call's result (#14206).

```expected-classes
probe:src_lib.rs:predicate:63b83c61 -> weakly_exposed
probe:src_lib.rs:return_value:23a22a72 -> weakly_exposed
```

## Must Not

- Read `exposed` or a strong oracle from an exact assertion whose compared
  value discards the owner call's result.
- Claim a runtime mutation outcome; this is static evidence only.
