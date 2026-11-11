# Fixture: typescript_generator_promise

Spec: RIPR-SPEC-0243

Issue: #8286

## Given

The exported async `total` resolves to a changed number. The test returns
`total().then(function* (sum) { assert.strictEqual(sum, 30); })`. A native
promise calls the generator callback only to create an iterator, so the
callback body never runs and the strict equality is never evaluated.

## When

From `input`, run `npm test` (`node --test tests/total.test.ts`, Node 22.18 or
later for built-in type stripping; no dependencies). Repeat with `return 31`
in `src/total.ts`. Both runs execute one passing test. Removing the `*` from
the wrong-value variant makes that same assertion fail. The production
analyzer command is:

```bash
ripr check --root input --diff ../diff.patch --format json --mode fast
```

## Then

One changed return-value finding on line 2 reads `static_unknown`: the only
assertion-like code sits in a callback the extractor refuses (RIPR-SPEC-0246
rule 4). The related test remains present with an unknown oracle. Withheld
assertion-like syntax stays unresolved; it must not be presented as
established absence. The independent evidence-promotion-honesty row prohibits promotion
even if a wrong golden is blessed.

The expected static class per finding (machine-checked against
`expected/check.json` by `check-fixture-contracts`, #12928):

```expected-classes
probe:src_total.ts:typescript_preview:44386250 -> static_unknown
```

## Must Not

- Supply exposure credit from a returned or awaited `.then(function* ...)` or
  `.then(async function* ...)` callback body, with zero or one parameter.
- Remove credit from an ordinary or async function or arrow `.then` callback,
  or from an independent assertion beside a dormant generator callback.
- Claim general generator execution, iterator consumption or promise type
  analysis.
- Promote TypeScript's support tier or claim mutation adequacy.
