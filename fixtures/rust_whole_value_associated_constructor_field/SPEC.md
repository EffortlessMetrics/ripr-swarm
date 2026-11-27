# Fixture: rust_whole_value_associated_constructor_field

Spec: RIPR-SPEC-0225

## Given

`Cfg` derives `PartialEq`. Its associated constructor `Cfg::new()` returns the literal `Cfg { retries: 4, verbose: false }`, changed from `retries: 3`. The test imports `Cfg` directly and runs `assert_eq!(Cfg::new(), Cfg { retries: 3, verbose: false })`.

## When

`cargo xtask fixtures rust_whole_value_associated_constructor_field` runs the public analyzer.

## Then

The field_construction finding reads `exposed`. The whole-value literal names `retries` with an independent literal, and the derived equality compares it, so the test notices the change (#15345). The associated call is admitted through the same `T::name(..)` identity a return value uses (RIPR-SPEC-0172).

## Must Not

- Read `weakly_exposed` because the owner is an associated function rather than a free function or method.
- Require the expected literal to spell the head initializer text.
