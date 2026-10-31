# Fixture: python_boundary_local_input_credited (a single-assignment local places the boundary input)

Spec: RIPR-SPEC-0028

## Given

The predicate `score > 50` becomes `score >= 50` in `tier`. The tests are one
strong test at `tier(80)` and an edge test that reaches the boundary through a
test-local name:

```python
def test_edge():
    s = 50
    assert tier(s) == "basic"
```

## When

`ripr check` analyzes the diff against the Python preview adapter.

## Then

ripr reads the call as `tier(50)` (#10707): the changed predicate boundary `score == 50` has an input and the strong exact-value oracle observes it, so the finding is `exposed`, the same as the literal call.

## Must Not

- Credit a local that is bound more than once, conditionally, after the call,
  to a non-literal, or that another scope's name shadows.
- Run any Python runtime; static preview evidence only.
