# Fixture: observation_verified_side_effect

Spec: RIPR-SPEC-0094

## Given

Production code changes a `process_order` function to pass `order_id` (instead
of a static literal) as the payload to `notifier.send`:

```rust
pub fn process_order(notifier: &Notifier, order_id: &str) -> bool {
    notifier.send(order_id)   // ← was notifier.send("static_msg")
}
```

The `Notifier` records every payload it sends, and the related test asserts the
exact recorded payload with `assert_eq!` — a Strong exact-value observer that
also names `notifier` (a token from the probe expression):

```rust
#[test]
fn process_order_sends_exact_order_id() {
    let notifier = Notifier::new();
    process_order(&notifier, "order-42");
    assert_eq!(*notifier.sent.borrow(), vec!["order-42".to_string()]);
}
```

This assertion both (a) token-matches the probe expression
`notifier.send(order_id)` and (b) reaches Strong oracle strength, so the
discriminate stage is `yes` and the finding is `exposed`.

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

The diff's new-side call must coincide with `input/src/lib.rs:18`. The call
is a tail-return expression without a semicolon: the real parser inventory is
one SideEffect and one ReturnValue probe, with no CallDeletion probe. The
persisted-payload observer keeps SideEffect `exposed` without
`observation_unverified`. ReturnValue is `weakly_exposed`: the test does not
assert the returned boolean.

The fixture runner rejects a shifted new-side coordinate before analysis.
Its regression controls shift the retained hunk one line early and one line
late, reject an independently shifted old side, and independently pin the real
public analyzer's two-family inventory and classifications.
Standalone-call deletion coverage belongs to
[`observation_verified_call_deletion`](../observation_verified_call_deletion/SPEC.md).

This fixture is the **verified-effect control** for
[RIPR-SPEC-0094](../../docs/specs/RIPR-SPEC-0094-observation-unverified-guard-generalization.md):
an effect probe whose changed behavior is genuinely observed (strong persisted-
state assertion) stays `exposed`, proving the `observation_unverified` guard
does not over-correct legitimately-observed effects.

## Must Not

- Emit `observation_unverified` when the assertion observes the changed effect.
- Downgrade a strong, token-matching effect observer to `weakly_exposed`.
- Claim CallDeletion coverage for this tail-return expression.
- Use mutation-runtime outcome vocabulary.
