# Fixture: python_boundary_local_rebound_not_credited (a rebound local does not place the boundary input)

Spec: RIPR-SPEC-0028

## Given

The predicate `score > 50` becomes `score >= 50` in `tier`. The tests are one
strong test at `tier(80)` and an edge test that reaches the boundary through a
test-local name:

```python
def test_edge():
    s = 50
    s = 60
    assert tier(s) == "basic"
```

## When

`ripr check` analyzes the diff against the Python preview adapter.

## Then

The name is bound twice, so its value at the call is not settled by one literal (#10707). The finding stays `weakly_exposed` and the missing discriminator `score == 50` is still named.

## Must Not

- Credit a local that is bound more than once, conditionally, after the call,
  to a non-literal, or that another scope's name shadows.
- Run any Python runtime; static preview evidence only.
