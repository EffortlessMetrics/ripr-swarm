# Fixture: compile_time_assert_stop_reason

Spec: RIPR-SPEC-0002

## Given

A crate changes `pub const N` from 8 to 12 and its compile-time assertion
`const _: () = assert!(N <= 16)` to `N <= 12` (#11547).

## When

`cargo xtask fixtures compile_time_assert_stop_reason` runs the public analyzer.

## Then

The assertion line is `static_unknown` with stop reason `compile_time_invariant`
and not `no_changed_rust_line`. The `pub const N` line keeps its existing
reason. Neither finding is `exposed`: a violating edit fails to build, which is
not a test kill.

## Must Not

- Report `no_changed_rust_line` for a changed Rust compile-time assertion.
- Credit the assertion as `exposed` from a test.
