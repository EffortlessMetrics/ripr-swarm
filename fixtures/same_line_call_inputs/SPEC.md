# Fixture: same_line_call_inputs

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #13373

## Given

The owner changes its above-boundary return value. A test asserts the above-boundary
call followed by the boundary call on one source line. These intentionally analyzed
assertions are fixture input; the first expected result represents the prior behavior.

## When

```bash
cargo xtask fixtures same_line_call_inputs
```

## Then

Both `weight = 101` and `weight = 100` are activation evidence. The predicate must
not claim that no related test supplies `weight == 100`.

## Must Not

- Read only the first call on the source line.
- Credit a same-named sibling, method, string or comment as an owner input.
- Infer an exact value for an unreadable nested call argument.
