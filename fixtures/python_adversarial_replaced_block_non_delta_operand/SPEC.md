# Fixture: python_adversarial_replaced_block_non_delta_operand (false-exposed guard: a block's later line pairs its own old line)

Spec: RIPR-SPEC-0028

## Given

The non-delta-operand trap of `python_adversarial_changed_sink_non_delta_operand`
(#1276) without its `@property`, placed as the second line of a two-line
replacement block (#7556). The owner is a plain method the test calls, so the
test is related and reach holds. The comment above the return and the return
are rewritten together:

```python
# changed owner: Account.balance  (delta: wrap in max(0, ...))
def balance(self):
    # the amount on record, never below zero
    return max(0, self._balance)

# the only test calls balance() but asserts the UNCHANGED operand `_balance`
def test_balance_starts_at_the_opening_amount():
    account = Account(100)
    account.balance()
    assert account._balance == 100
```

Both removed lines share the new-side line where the added run starts. A
coordinate-only lookup therefore finds an old line only for the comment, and
the return is compared against nothing, so every token on it, `_balance`
included, reads as delta.

## When

`ripr check` analyzes the diff against the Python preview adapter.

## Then

The return pairs with its own old line (`return self._balance`), the only delta
token is `max`, and the oracle on `_balance` does not observe it. The change
reads `weakly_exposed`, as the same clamp does as a one-line edit. Running the test with
the clamp applied passes (labeled in the Python verdict corpus as
`py-blocks-account-clamp-second-line`).

**This fixture must NEVER read `exposed`.**

## Must Not

- Credit `exposed` from a `changed_sink_token` match on an operand that is
  unchanged between the paired old and new line.
- Run any Python runtime; static preview evidence only.
