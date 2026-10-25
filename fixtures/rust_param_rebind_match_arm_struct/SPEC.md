# Fixture: rust_param_rebind_match_arm_struct

Spec: RIPR-SPEC-0158

## Given

`score` compares `m == 40` where `m = n * 2`, and the parameter `n` is rebound
by a match-arm struct pattern `Some(Foo { n, .. })` before that `let`. The related integration test passes `20`,
which would sit on the boundary only if `n` still held the caller's value (#9774, #10154).

## When

`ripr check` analyzes the changed comparison (`39` -> `40`).

## Then

The finding is not `exposed`: the rebinding means the test input `20` never reaches `m`.

## Must Not

Credit the boundary from the test input `20` through the rebound name.
