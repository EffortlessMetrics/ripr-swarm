# Fixture: ts_dead_assertion_after_return

Spec: RIPR-SPEC-0243

## Given

A TypeScript owner `isEligible(age)` changes its boundary predicate from
`age >= 18` to `age > 18`. The only related test makes a weak live
assertion, `expect(isEligible(30)).toBeTruthy()`, then executes an
unconditional `return;`. The one exact boundary assertion,
`expect(isEligible(18)).toBe(false)`, sits after that `return;` in the test
body, so it never runs.

The fixture workspace enables the TypeScript preview adapter via
`ripr.toml`:

```toml
[languages]
enabled = ["rust", "typescript"]
```

## When

```bash
ripr check \
  --root fixtures/ts_dead_assertion_after_return/input \
  --diff fixtures/ts_dead_assertion_after_return/diff.patch \
  --mode fast
```

## Then

The TypeScript preview adapter:

- treats the statement after the unconditional `return;` as dead code and
  withholds its assertion from oracle collection,
- keeps the finding `weakly_exposed`, crediting only the live
  `toBeTruthy()` assertion.

## Must Not

- Promote the finding to `exposed` because a strong exact assertion appears
  in the test body (the confirmed false promotion, #9003).
