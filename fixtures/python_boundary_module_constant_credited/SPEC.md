# Fixture: python_boundary_module_constant_credited (a test-module constant places the boundary input)

Spec: RIPR-SPEC-0028

## Given

The predicate `score > 50` becomes `score >= 50` in `tier`. The tests are one
strong test at `tier(80)` and an edge test that reaches the boundary through a
constant of the test file, bound once at module scope:

```python
THRESHOLD = 50


def test_edge():
    assert tier(THRESHOLD) == "basic"
```

## When

`ripr check` analyzes the diff against the Python preview adapter.

## Then

ripr reads the call as `tier(50)` (#10707, RIPR-SPEC-0028): the changed predicate boundary `score == 50` has an input and the strong exact-value oracle observes it, so the finding is `exposed`, the same as the literal call.

## Must Not

- Credit a module constant that is bound more than once, conditionally, to a
  non-literal, or that a `global` write, attribute write, patch or the test
  itself can replace.
- Run any Python runtime; static preview evidence only.
