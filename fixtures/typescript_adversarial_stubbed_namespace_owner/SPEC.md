# Fixture: typescript_adversarial_stubbed_namespace_owner (false-exposed guard — a hook stubs the changed owner on its module object)

Spec: RIPR-SPEC-0243

Corpus case: `ts_stubbed_namespace_owner_observes_the_stub` in
`fixtures/evidence-promotion-honesty-corpus/corpus.json` (issue #15939).

## Given

The changed owner is the exported function `applyDiscount` (src/discount.ts),
whose threshold predicate changes from `amount > threshold` to
`amount >= threshold`. The only test file imports the owner's module as a
namespace, replaces the owner in a `beforeEach` with a sinon stub, and asserts
an exact value on the stubbed call:

```ts
import sinon from "sinon";
import * as pricing from "../src/discount";

beforeEach(() => {
    sinon.stub(pricing, "applyDiscount").returns(90);
});

test("applyDiscount at threshold discounts", () => {
    expect(pricing.applyDiscount(100, 100)).toBe(90);
});
```

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`.

## When

```bash
ripr check \
  --root fixtures/typescript_adversarial_stubbed_namespace_owner/input \
  --diff fixtures/typescript_adversarial_stubbed_namespace_owner/diff.patch \
  --mode fast
```

## Then

The finding reads `weakly_exposed`. `owner_function_replaced_in_file`
(`crates/ripr/src/analysis/language/typescript/function_replacement.rs`)
withholds the owner-call relation in `owner_call_relation`, so the only link
left is the same-file-stem proximity heuristic, which cannot borrow the
exact-value assertion. The finding names
`typescript_spy_fabricated_observer`, and `repair_packet_ready` stays `false`.

Before #15939 this read `exposed` with `relation_reason:
import_path_affinity` and a strong `exact_value` oracle.

## Must Not

- Read `exposed`: the call in the test runs the stub, not the changed
  comparison.
- Credit an owner-call relation through a test file that replaces the owner.
- Lower the call-through control
  (`fixtures/typescript_namespace_owner_call_through_spy`), whose spy has no
  fabrication.
- Run any TypeScript runtime; static preview evidence only.

The expected class is pinned per finding (`expected/check.json`, checked by
`check-fixture-contracts`):

```expected-classes
probe:src_discount.ts:typescript_preview:2396aec1 -> weakly_exposed
```

Re-bless this fixture on Linux when the classifier wording or the digest changes:
`cargo xtask goldens bless typescript_adversarial_stubbed_namespace_owner --reason "RIPR-SPEC-0243:
<reviewed change>"`, which records the reason in this fixture's `expected/CHANGELOG.md`.
