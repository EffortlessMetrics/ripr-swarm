# Fixture: python_boundary_module_constant_rebound_not_credited (a rebound test-module constant is not credited)

Spec: RIPR-SPEC-0028

## Given

The predicate `score > 50` becomes `score >= 50` in `tier`. The tests are one
strong test at `tier(80)` and an edge test that reaches the boundary through a
module constant the test file binds twice:

```python
THRESHOLD = 50
THRESHOLD = 49


def test_edge():
    assert tier(THRESHOLD) == "basic"
```

## When

`ripr check` analyzes the diff against the Python preview adapter.

## Then

ripr does not read the constant as a literal input (#10707, RIPR-SPEC-0028): the module binds it twice, so the value at the call is not established and no input is placed at the changed boundary `score == 50`. The finding stays `weakly_exposed`.

## Must Not

- Credit a constant the module binds more than once.
- Run any Python runtime; static preview evidence only.
