# Fixture: inherent_receiver_module_type_shadow

Spec: RIPR-SPEC-0005

## Given

A private inherent owner `JsonValue::as_object` in the crate root, and a
`#[cfg(test)]` module that declares its own module-scoped `JsonValue` type with
a same-named `as_object` method (independent review #8004).

## When

Analyze the changed Object arm and complete repository. Compile the native source,
then replace the private Object arm with `None` and rerun its tests.

## Then

The original and owner-mutant tests pass, because the only test exercises the
shadowing module-local type. Both probes (match arm and return value) are
retained at low relation reach with class `reachable_unrevealed`. All four owner
seams remain present; full and compact relation projections agree. The paired
`controls/genuine-owner.rs` positive, where the test uses the real owner type,
rejects the owner mutant.

## Must Not

Promote the module-scoped shadowing `JsonValue` type, which shares only a name
with the owner, into executed owner evidence. Do not rewrite runtime truth to
fit advice.

## Regression Controls

The adjacent `controls/` files are complete native Rust subjects. A shared
owner-identity boundary matrix and production-path integration proofs are
intended to land in the integration-tests slice (#11031) and the sibling fixture
slice; this fixture slice does not carry them.
