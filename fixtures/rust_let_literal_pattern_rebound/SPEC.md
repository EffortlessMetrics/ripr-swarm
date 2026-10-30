# Fixture: rust_let_literal_pattern_rebound

Spec: RIPR-SPEC-0158
Spec: RIPR-SPEC-0108

## Given

A test binds `let n = 10;`, then rebinds `n` through the tuple pattern `let (n, _m) = (9, 1);` before calling `gate(n)`. The call really receives `9`.

## When

`gate` changes `x >= 10` to `x > 10`. Only the input `10` tells the two apart.

## Then

The finding stays `weakly_exposed`: the call's argument is not the literal `10`.

## Must Not

Credit the test as discriminating the boundary by reading the shadowed `let n = 10` (#10819). The honesty corpus independently rejects a false promotion after re-blessing.
