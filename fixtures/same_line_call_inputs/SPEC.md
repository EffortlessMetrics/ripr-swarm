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

The edit changes only the inline branch value (`50` to `60`); `weight > 100`
is unchanged, so the finding is the `return_value` probe for `60`, and the
same-line pin `shipping(101) == 50` exposes it (#15529). No boundary row asks
for `weight == 100`. Reading every same-line call as a boundary input for a
changed predicate stays pinned by `crates/ripr/tests/same_line_call_inputs.rs`,
on the analogous owner `band` whose condition `n >= 10` becomes `n > 9`.

## Must Not

- Read only the first call on the source line.
- Credit a same-named sibling, method, string or comment as an owner input.
- Infer an exact value for an unreadable nested call argument.
