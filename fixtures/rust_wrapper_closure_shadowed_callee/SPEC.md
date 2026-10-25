# Fixture: rust_wrapper_closure_shadowed_callee

Spec: RIPR-SPEC-0186

Helper result forwarding follows RIPR-SPEC-0159.

## Given

A private boolean predicate changes from `qty >= 10` to `10 <= qty`. Its public
wrapper binds a local closure named `is_bulk` and calls that closure, so the
wrapper never calls the changed function. One integration test pins the
wrapper at 10 and 9.

## When

`ripr check` analyzes the predicate's changed line and its whole-return twin.

## Then

Neither finding is `exposed`: a caller that rebinds its callee's name calls
the binding, not the helper (#10284).

## Must Not

Promote either finding from the wrapper's exact assertions. Any mutation of
the private predicate leaves the wrapper's result unchanged.
