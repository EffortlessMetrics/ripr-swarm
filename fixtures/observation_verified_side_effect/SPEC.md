# Fixture: observation_verified_side_effect

Spec: RIPR-SPEC-0094

## Given

Production code changes a `process_order` function to pass `order_id` as the
payload to `notifier.send`:

```rust
pub fn process_order(notifier: &Notifier, order_id: &str) -> bool {
    notifier.send(order_id)   // ← was notifier.send("static_msg")
}
```

The related test exercises `process_order` AND also directly calls
`notifier.send(sent_order_id)` in its assertions, referencing `order_id` via a
local variable `sent_order_id`:

```rust
#[test]
fn process_order_sends_order_id() {
    let notifier = Notifier;
    let sent_order_id = "order-42";
    let result = process_order(&notifier, sent_order_id);
    assert!(notifier.send(sent_order_id));
}
```

The assertion `assert!(notifier.send(sent_order_id))` contains "sent_order_id",
which as a substring contains "order_id" — a token from the probe expression
`notifier.send(order_id)`. Therefore `token_match` fires and
`observation_unverified` is NOT set.

## When

```bash
cargo xtask fixtures observation_verified_side_effect
```

or:

```bash
ripr check --root fixtures/observation_verified_side_effect/input \
           --diff fixtures/observation_verified_side_effect/diff.patch --mode fast
```

## Then

`ripr` must NOT emit `observation_unverified` for the SideEffect probe.
The discriminate summary must NOT contain `observation_unverified` — it may
be weak from oracle strength (mock/expectation yields Medium), but the
weakness must come from the oracle strength path, not the token-match guard.

This fixture is the **anti-over-correction proof** for the SideEffect family
in [RIPR-SPEC-0094](../../docs/specs/RIPR-SPEC-0094-observation-unverified-guard-generalization.md):
a SideEffect probe with a token-matching assertion must NOT be downgraded by
`observation_unverified`.

## Must Not

- Emit `observation_unverified` when the assertion contains a token from the changed call.
- Use mutation-runtime outcome vocabulary.
