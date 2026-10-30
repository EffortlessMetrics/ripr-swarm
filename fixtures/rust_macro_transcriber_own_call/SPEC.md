# Fixture: rust_macro_transcriber_own_call

Spec: RIPR-SPEC-0120
Spec: RIPR-SPEC-0108

## Given

A test-local `check_small!` macro whose transcriber asserts `bulk(5)` against its argument, called as `check_small!(false)`. The input `5` comes from the macro, not from the call line.

## When

`bulk` changes `n > 100` to `n >= 100`. Only the input `100` tells the two apart.

## Then

The finding stays `weakly_exposed`: the written-out assertion `assert_eq!(bulk(5), false)` reads the same way.

## Must Not

Expand the macro and grade the exact pin against the call line's inputs, which promoted the finding to `exposed` (#10854). The honesty corpus independently rejects a false promotion after re-blessing.
