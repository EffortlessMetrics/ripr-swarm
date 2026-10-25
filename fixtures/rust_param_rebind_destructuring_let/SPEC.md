# Fixture: rust_param_rebind_destructuring_let

Spec: RIPR-SPEC-0158

## Given

`score` compares `m == 40` where `m = n * 2`, and the parameter `n` is rebound
by a destructuring `let (n, _) = (7, 0)` before that `let`. The related integration test passes `20`,
which would sit on the boundary only if `n` still held the caller's value (#9774, #10154).

## When

`ripr check` analyzes the changed comparison (`39` -> `40`).

## Then

The finding is not `exposed`: the rebinding means the test input `20` never reaches `m`.

## Must Not

Credit the boundary from the test input `20` through the rebound name.
