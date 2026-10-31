# Fixture: python_noop_trailing_comment (no-op guard — a trailing \`#\` comment has no behavior delta)

Spec: RIPR-SPEC-0028

## Given

A Python diff whose only change is a trailing \`#\` comment on the `return` line of `discount`.
The statement is token-for-token the same code, and the related test is a
strong exact-value oracle on the owner's output:

```python
# changed owner: src/pricing.py::discount
def discount(price):
    return price * 0.8  # fallback   # was: "    return price * 0.8"
```

```python
from src.pricing import discount

def test_discount():
    assert discount(100) == 80
```

## When

`ripr check` analyzes the diff against the Python preview adapter.

## Then

ripr emits **no behavior probe** (#10706): the line runs identically, so there
is nothing for a test to discriminate and no gap id or discriminator can embed
the comment text. The finding set is empty.

## Must Not

- Emit a finding, gap id or discriminator for a comment-only or spacing-only
  change to a line.
- Treat a changed token, a respelling or a changed indentation as a no-op.
- Run any Python runtime; static preview evidence only.
