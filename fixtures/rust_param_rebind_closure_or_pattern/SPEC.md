# Fixture: rust_param_rebind_closure_or_pattern

Spec: RIPR-SPEC-0158

## Given

`score` compares `m == 40` where `m = n * 2`, but the closure parameter pattern `(Either::Left(n) | Either::Right(n))` binds its own `n`, and the compared local `m = n * 2` is computed inside the closure. The inner `n` shadows the
parameter there, so the related integration test's input `20` never reaches `m`
(#9774, #10154). The test pins the real result (`1`).

## When

`ripr check` analyzes the changed comparison (`39` -> `40`).

## Then

The finding is not `exposed`: the boundary operand `m` is not mapped to the test input `20`.

## Must Not

Credit the boundary from the test input `20` through the rebound name.
