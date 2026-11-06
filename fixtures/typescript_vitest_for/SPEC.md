# Fixture: typescript_vitest_for

Spec: RIPR-SPEC-0243

Issue: #10760

## Given

A Vitest 4.1.11 literal table passes each array row whole to parameter one,
with TestContext in parameter two. Destructured rows use the context's
`expect`; indexed rows use `ctx.expect`. The changed owner moves the score
50 boundary from `>` to `>=` while the unchanged test expects `basic`.

## When

Run `ripr check --root input --diff ../diff.patch --format json --mode fast`.
For independent native evidence, run `npm ci --ignore-scripts` and `npm test`
inside `input`. The retained input contains the changed owner, so native tests
fail at the two score-50 assertions. Restore `>` for the passing baseline.

Observed on Linux, Node 24.19.0, npm 11.9.0, Vitest 4.1.11, 2026-11-06:

| Frozen test suite / owner | Native exit | Tests | Passed | Failed | Skipped |
| --- | --- | --- | --- | --- | --- |
| Baseline `score > 50` | 0 | 8 | 7 | 0 | 1 |
| Changed owner `score >= 50` | 1 | 8 | 5 | 2 | 1 |
| Restored `score > 50` | 0 | 8 | 7 | 0 | 1 |

The two failures report `expected 'gold' to be 'basic'` in the destructured
and indexed score-50 rows. This is assertion failure after actual registration,
not a compile failure or an empty run. `test.for([])` adds no subject;
`test.for([[]])` adds one. The second-context skip is observed as skipped.
The package lock pins the dependency graph used for these runs.

## Then

The boundary finding is `exposed` through the two supported row/context
profiles. JSON and human output come from the shared finding producer.
The bare empty-array value assertion and context skip are deliberately outside
this bounded extraction profile and retain named partial-extraction disclosure;
they supply no static discriminator. Independent useful rows retain evidence.

## Must Not

- Spread an array row across callback parameters as `.each` would.
- Treat row data as the TestContext or credit assertions after context skip.
- Infer positive semantics for dynamic, object, spread, tagged or exhausted rows.
- Claim general Vitest support, automatic execution, mutation adequacy, repair
  readiness, release membership or any TypeScript support-tier promotion.

## Native input binding

- `input/package-lock.json` SHA-256: `3f0c1e62abe3bdfee7e0f3f8c2554e3add9b164a446924512a613fce3a5e809b`
- `input/src/tier.ts` SHA-256: `c7d5a4985ad73698dce7f5076b1558f2d02dda618b66710b04ed169d5f310bf6`
- `input/tests/tier.test.ts` SHA-256: `415eb7636e0eadb8eae66670338f0fea2d098e085f30a05168eca57ca9925c4f`
