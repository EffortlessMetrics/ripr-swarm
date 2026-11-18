# Fixture: rust_macro_declared_flags_vec_contains

Spec: RIPR-SPEC-0108
Issue: #11113

## Given

A bitflags-shaped crate: `macro_rules! test_flags` declares `struct TestFlags` and spells `impl $crate::Flags for TestFlags`. The test `cases` builds a `Vec<(&str, TestFlags)>` and asserts `value.contains(..)`, which is `slice::contains`.

## When

The trait default method `Flags::contains` changes its comparison from `==` to `!=`.

## Then

The finding for the changed predicate is not `exposed` or `weakly_exposed`, and `cases` is related by name only (`weak_token_substring`). The `Vec` receiver cannot be the trait's receiver, because the macro implements the trait only for the type it declares.

## Must Not

- Read `Vec::contains` as a direct call of `Flags::contains` because a macro in the crate spells `impl`, the trait name and `for` (#11113).
- Accept a missing finding or a missing related test as a non-promotion pass.
