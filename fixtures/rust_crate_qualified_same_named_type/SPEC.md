# Fixture: rust_crate_qualified_same_named_type

Spec: RIPR-SPEC-0005

Owner: analysis-fixtures

Issue: #14083

## Given

The library has two modules with a type named `Rng`: `src/lib.rs` is
`mod a; mod b; pub use b::Rng;`. The changed predicate is in `a::Rng::digit`,
and `a` has no constructor. The one integration test calls
`rust_crate_qualified_same_named_type_fixture::Rng::new().digit(..)`, which
builds the re-exported `b::Rng` and never runs the changed method.

## When

`cargo xtask fixtures rust_crate_qualified_same_named_type` runs the public diff analyzer.

## Then

The predicate finding reads `reachable_unrevealed`. The test is related only
by a weak token match, not a direct owner call, because the crate-qualified
constructor route needs exactly one type of that name in the workspace (#14083
A2). The same layout with one `Rng` reads `exposed` through a direct owner
call.

```expected-classes
probe:src_a.rs:predicate:4e378913 -> reachable_unrevealed
```

## Must Not

- Credit the test with a direct owner call, or read `exposed`, from a
  crate-qualified receiver that names another module's same-named type.
- Claim a runtime mutation outcome; this is static evidence only.
