# Fixture: typescript_namespace_owner_call_through_spy (control — a spy with no fabrication still runs the changed owner)

Spec: RIPR-SPEC-0243

Corpus case: `ts_namespace_owner_call_through_spy_keeps_credit` in
`fixtures/evidence-promotion-honesty-corpus/corpus.json` (issue #15939).

## Given

The same owner and diff as
`fixtures/typescript_adversarial_stubbed_namespace_owner`. The test file spies
on the owner in a `beforeEach` without fabricating a value, so the call runs
the changed code:

```ts
import * as pricing from "../src/discount";

let spy: jest.SpyInstance;

beforeEach(() => {
    spy = jest.spyOn(pricing, "applyDiscount");
});

test("applyDiscount at threshold discounts", () => {
    expect(pricing.applyDiscount(100, 100)).toBe(90);
    expect(spy).toHaveBeenCalledTimes(1);
});
```

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`.

## When

```bash
ripr check \
  --root fixtures/typescript_namespace_owner_call_through_spy/input \
  --diff fixtures/typescript_namespace_owner_call_through_spy/diff.patch \
  --mode fast
```

## Then

The finding reads `exposed` through `import_path_affinity` with the strong
`exact_value` oracle `toBe(90)` at the boundary input `(100, 100)`. The
replaced-owner guard (#15939) records nothing for a call-through spy.

## Must Not

- Withhold the relation because the file mentions `spyOn` on the owner.
- Name `typescript_spy_fabricated_observer`.

The expected class is pinned per finding (`expected/check.json`, checked by
`check-fixture-contracts`):

```expected-classes
probe:src_discount.ts:typescript_preview:2396aec1 -> exposed
```

Re-bless this fixture on Linux when the classifier wording or the digest changes:
`cargo xtask goldens bless typescript_namespace_owner_call_through_spy --reason "RIPR-SPEC-0243:
<reviewed change>"`, which records the reason in this fixture's `expected/CHANGELOG.md`.
