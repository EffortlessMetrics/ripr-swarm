# TypeScript: discount threshold (preview)

TypeScript analysis is a preview: its evidence is advisory and is not on the
same scale as Rust evidence. No `ripr.toml` is needed; `ripr` enables the
preview because `project/` has a `tsconfig.json`.

`project/src/pricing.ts` gives a discount to orders at or above
`DISCOUNT_THRESHOLD`. [`change.diff`](change.diff) is the change that made the
boundary inclusive (`>` became `>=`). The tests in
`project/test/pricing.test.ts` check 9999 and 10001, so they pass with either
comparison.

## See the finding

From this directory:

```bash
ripr check --root project --diff change.diff
```

Selected lines of the output:

```text
Start here:
  State: preview language, advisory only
  File: src/pricing.ts:5
  Language: typescript
  Static exposure: weak (severity: warning, confidence 0.40)
  Changed behavior: if (subtotal >= DISCOUNT_THRESHOLD) {
  Missing check (1 of 2): Related tests reach `discountedTotal` with a strong oracle, but no strong
    assertion observes a `discountedTotal(...)` call at the changed predicate boundary `subtotal ==
    DISCOUNT_THRESHOLD`; inputs on one side of the boundary do not discriminate the changed
    comparison
```

## Fix it

[`strengthened/pricing.test.ts`](strengthened/pricing.test.ts) is the same test
file with one more case, which asserts `discountedTotal(10000)` is `9500`. Put
it in place and check again:

```bash
cp strengthened/pricing.test.ts project/test/pricing.test.ts
ripr check --root project --diff change.diff
```

The summary now reads `1 appear noticed by tests` and the finding is `exposed`.
To run the tests themselves: `cd project && npm install && npm test`.

Restore the starting point with
`git checkout -- project/test/pricing.test.ts`.
