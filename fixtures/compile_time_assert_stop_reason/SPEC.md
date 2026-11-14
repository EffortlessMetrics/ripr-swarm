# Fixture: compile_time_assert_stop_reason

Spec: RIPR-SPEC-0002

## Given

A crate changes `pub const N` from 8 to 12 and its compile-time assertion
`const _: () = assert!(N <= 16)` to `N <= 12` (#11547). It also changes the
continuation line of a multi-line `const _: () = assert!(\n    W <= 4\n);`, and
an unrelated `pub const FREE` that no assertion names (#12040).

## When

`cargo xtask fixtures compile_time_assert_stop_reason` runs the public analyzer.

## Then

The assertion line is `static_unknown` with stop reason `compile_time_invariant`
and not `no_changed_rust_line`. The `pub const N` line, which that same-file
assertion names, and the multi-line assertion's changed continuation line carry
`compile_time_invariant` too (#12040). `pub const FREE` keeps
`no_changed_rust_line`. No finding is `exposed`: a violating edit fails to build, which is
not a test kill.

## Must Not

- Report `no_changed_rust_line` for a changed Rust compile-time assertion.
- Credit the assertion as `exposed` from a test.
- Report `no_static_path` for a continuation line of a compile-time assertion.
- Link a const to an assertion that does not name it, or whose name is declared twice in the file.
