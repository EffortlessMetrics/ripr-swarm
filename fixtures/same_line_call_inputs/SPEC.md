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

The edit changes only the inline branch value (`50` to `60`); the predicate
`weight > 100` is unchanged, so the line's finding is the changed returned
value `60` (`return_value`), not the predicate's boundary. Same-line inputs for
a changed predicate stay covered by `crates/ripr/tests/same_line_call_inputs.rs`.

## Must Not

- Read only the first call on the source line.
- Credit a same-named sibling, method, string or comment as an owner input.
- Infer an exact value for an unreadable nested call argument.
