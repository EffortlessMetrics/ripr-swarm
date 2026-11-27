# Fixture: compile_error_build_time

Spec: RIPR-SPEC-0411

## Given

A crate changes the message of a one-line `compile_error!(..)` and the message
line of a `compile_error!(` invocation split over three lines, each under a
`#[cfg(..)]` attribute (#15933).

## When

`cargo xtask fixtures compile_error_build_time` runs the public analyzer.

## Then

Each changed line is one `static_unknown` finding with stop reason
`compile_time_invariant`. The split invocation's message line carries it too,
although the line holds only the message.

## Must Not

- Report a changed `compile_error!` as a deleted call or a log side effect.
- Report `no_changed_rust_line` for the message line of a split invocation.
- Credit either line as `exposed` from a test.
