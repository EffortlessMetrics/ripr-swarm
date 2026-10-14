# Fixture: rust_commented_module_orphan

Spec: RIPR-SPEC-0153

## Given

A real Cargo package declares `#[cfg(test)] mod tests;`. The module file
contains `mod used_tests;` only inside a nested block comment. A nested
fixture file pins `discount(100)` to the independent literal `100`, but
no target's module tree compiles it. An unrelated integration target
contains `integration_is_collected` so the executable test set is nonempty.

## When

`ripr check` analyzes the predicate change in `src/used.rs` while the
test files remain unchanged.

## Then

Exactly one production predicate finding has classification
`no_static_path` and an empty related-test list. Cargo 1.95.0
`test --all-targets -- --list` lists exactly `integration_is_collected`;
it lists no unit tests and never lists `discount_applies`.

The public `rust_discovery_comments` integration tests separately retain
ordinary `cfg(test)` declarations and a renamed module loaded through
`#[path = "data/used_tests.rs"]`. They also retain credit when an opaque
macro makes the authoritative module walk incomplete.

## Must Not

- Credit a declaration mentioned only in a comment or string.
- Treat the unrelated integration test as reaching `discount`.
- Change unknown module-walk results into a proof of absence.
- Claim repo-scoped discovery, general macro expansion or cfg evaluation.
