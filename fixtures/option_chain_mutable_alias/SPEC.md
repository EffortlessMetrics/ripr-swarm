# Fixture: option_chain_mutable_alias

Spec: RIPR-SPEC-0158

Honesty invariant: RIPR-SPEC-0108.

## Given

A mutable reference replaces n with 2 after the option chain. Both literal rows therefore compare the overwritten value; the earlier initializer cannot authorize a later exact value.

## When

`cargo xtask fixtures option_chain_mutable_alias` analyzes the change from `n == 2` to
`n < 3`. Native Rust 1.95.0 executes both fixture tests successfully under
each comparison: these rows do not distinguish that alternative.

## Then

The changed predicate and returned comparison remain `infection_unknown`.
Neither receives a computed `n == 0` observation or an `exposed` verdict.
The honesty charters independently reject promotion and empty reports.

## Must Not

- Borrow builtin string semantics or a pre-overwrite value from literal proximity.
- Turn unavailable input identity into a missing-boundary prescription.
