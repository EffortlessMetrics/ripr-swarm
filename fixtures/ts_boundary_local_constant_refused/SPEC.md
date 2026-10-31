# Fixture: ts_boundary_local_constant_refused

Spec: RIPR-SPEC-0027

## Given

Ten TypeScript owners whose changed predicate `score > 50` becomes
`score >= 50` (issues #10707 and #10706), each with one related test that
reaches the owner through a name that is NOT a provably single immutable
integer binding:

- `letConstant`: `let edge = 50;`;
- `reassignedConstant`: `let edge = 50; edge = 49;`;
- `shadowedConstant`: a callback parameter `edge` shadows the module constant;
- `innerBlockConstant`: `const edge = 50` in an inner block, `const edge = 49`
  in the callback block;
- `computedConstant`: `const edge = 25 * 2;`;
- `importedConstant`: `edge` imported from a module the adapter cannot
  resolve to the owner;
- `mutatedConstant`: `config.edge` of an object whose property is rewritten;
- `offValueConstant`: `const edge = 60;`, a single constant off the boundary;
- `nestedClosureConstant`: `const edge = 50` inside a closure passed to a
  helper call, a module `const edge = 10` read outside it;
- `nestedFunctionConstant`: `const edge = 50` inside a nested function
  declaration, read outside it.

## When

```bash
ripr check \
  --root fixtures/ts_boundary_local_constant_refused/input \
  --diff fixtures/ts_boundary_local_constant_refused/diff.patch \
  --mode fast
```

## Then

Every finding stays `weakly_exposed`: the argument keeps its name and no
boundary witness is credited.

## Must Not

Credit any of these names; a wrong exposed result is worse than a missed one.
