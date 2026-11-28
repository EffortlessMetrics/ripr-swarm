# Fixture: typescript_generator_promise

Spec: RIPR-SPEC-0243
Spec: RIPR-SPEC-0108

Issue: #8286

## Given

The exported async `total` changes its returned value. The only test returns
`total().then(function* (sum) { assert.strictEqual(sum, 30); })`. A native
promise calls a `function*` callback only to create an iterator, so the
callback body, and the strict equality in it, never runs.

## When

From `input`, run `npm test` (`node --test tests/total.test.ts`, no
dependencies). Observed on Node 22.23.5 with the TAP reporter, one test per
run and none skipped: the test passes with `return 30` and still passes with
`return 31`. Replacing `function*` with `function` or `async function` makes
the `return 31` run fail at that strict equality, and the `return 30` run
pass. The production analyzer command is:

```bash
ripr check --root input --diff ../diff.patch --format json --mode fast
```

## Then

One changed `return_value` finding on `src/total.ts:2`, owner `total`, reads
`static_unknown`. The related test `total` stays present with an unknown
oracle. The withheld assertion-like syntax stays unresolved admission; it is
not presented as established absence. The independent
evidence-promotion-honesty row forbids promotion even if a wrong golden is
blessed.

## Must Not

- Supply exposure credit from the body of a generator or async generator
  `.then` callback.
- Remove credit from ordinary or async function and arrow `.then` callbacks,
  or from an independent assertion beside the dormant callback
  (`crates/ripr/tests/cli_smoke/typescript_generator_promise.rs`).
- Claim general generator execution, iterator consumption or promise type
  analysis.
- Promote TypeScript's support tier or claim mutation adequacy.
