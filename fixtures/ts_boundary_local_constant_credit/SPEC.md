# Fixture: ts_boundary_local_constant_credit

Spec: RIPR-SPEC-0027

## Given

Three TypeScript owners whose changed predicate `score > 50` becomes
`score >= 50` (issues #10707 and #10706). Each has one related test that
passes the boundary value `50` through a name instead of a literal:

- `moduleConstant`: a top-level `const EDGE_SCORE = 50;` in the test file;
- `moduleLowerConstant`: a top-level lowercase `const edge = 50;`;
- `localConstant`: `const edge = 50;` declared directly in the test callback.

Each name has exactly one binding, an integer literal initializer, and only
plain reads.

## When

```bash
ripr check \
  --root fixtures/ts_boundary_local_constant_credit/input \
  --diff fixtures/ts_boundary_local_constant_credit/diff.patch \
  --mode fast
```

## Then

All three findings are `exposed`: the name resolves to `50` by entity
identity, the value the changed comparison newly accepts.

## Must Not

Credit any shape in `ts_boundary_local_constant_refused`.
