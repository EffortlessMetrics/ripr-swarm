# Fixture: ts_strong_snapshot_untied

Spec: RIPR-SPEC-0027

## Given

A TypeScript method `Cart.add(amount)` changes `this.total += amount` to
`this.total += amount * 2` and returns `this.total`. The only related test calls
`cart.add(0)` and pins the result with a literal inline snapshot
(`toMatchInlineSnapshot`), a strong oracle. At `0` both versions add `0`, so the
snapshot holds before and after the edit.

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`.

## When

```bash
ripr check \
  --root fixtures/ts_strong_snapshot_untied/input \
  --diff fixtures/ts_strong_snapshot_untied/diff.patch \
  --mode fast
```

## Then

The related test's check is strong but ripr cannot tie it to the changed line, so the
next step says the check is already strong and asks for an input that flips the branch,
not for another exact-value assertion (#12463).

## Must Not

- Ask for another exact-value assertion alongside a strong inline snapshot.
- Report `exposed`, or emit an actionable repair packet.
