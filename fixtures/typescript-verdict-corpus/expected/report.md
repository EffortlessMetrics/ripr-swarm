# TypeScript verdict corpus report

Spec: RIPR-SPEC-0238. Corpus version: 2026-10-04.1. Cases: 101.

| Rate | Count | Rate |
| --- | --- | --- |
| False verdicts (all cases) | 36/101 | 0.3564 |
| False actionable (of discriminated) | 27/62 | 0.4355 |
| False exposed (of not fully discriminated) | 9/39 | 0.2308 |
| False silent (of not fully discriminated) | 0/39 | 0.0000 |
| Ideal verdict | 56/101 | 0.5545 |
| Abstained (limited or silent where acceptable) | 9/101 | 0.0891 |
| Findings with a contradiction | 0/101 | 0.0000 |

| Case | Origin | Truth | Ideal | Observed | Classes | Outcome | Changed since labeling | Contradictions |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `ts-nodetest-shipping-threshold` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `ts-nodetest-gold-threshold` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `ts-nodetest-silver-legacy-assert` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `ts-nodetest-gold-discount-rate` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `ts-nodetest-tax-self-computed` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `ts-nodetest-quantity-guard` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `ts-nodetest-audit-log` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `ts-nodetest-charge-rate` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `ts-nodetest-quote-shipping-term` | authored | not_discriminated | gap | credited | exposed | false_exposed | no | none |
| `ts-nodetest-footer-untested` | authored | not_discriminated | gap | limited | no_static_path | abstained | no | none |
| `ts-vitest-line-total-reexport` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `ts-vitest-bulk-threshold` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `ts-vitest-add-line-message` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `ts-vitest-remove-line-message` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `ts-vitest-is-empty` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `ts-vitest-checkout-notify` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `ts-vitest-unit-price-table` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `ts-vitest-summary-snapshot` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `jest-spec0234-ex29` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `jest-discount-boundary-pinned` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-shipping-boundary-unpinned` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-sku-method-chain` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-stock-truthy-only` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-quantity-tothrow-message` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-coupon-tothrow-broad` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-receipt-inline-snapshot` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `jest-cents-custom-matcher` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `jest-price-resolves-tobe` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-order-assert-in-then` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `jest-age-not-tobe` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-tax-module-mocked` | authored | not_discriminated | gap | limited | no_static_path | abstained | no | none |
| `jest-tier-test-each-table` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `jest-notify-called-with` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `jest-mined-default-export-object` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `jest-mined-test-fn-reference` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `jest-mined-suite-factory` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `jest-mined-private-impl-dispatcher` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `jest-mined-tomatchobject-field` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `jest-mined-tobeinstanceof` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `jest-mined-toequal-copy-not-identity` | authored | not_discriminated | gap | credited | exposed | false_exposed | no | none |
| `jest-mined-tobe-identity` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0234-ex28` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0234-ex01` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0234-ex02` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0234-ex03` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `jest-spec0234-ex04` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0234-ex05` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0234-ex06` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0234-ex07` | authored | not_discriminated | gap | credited | exposed | false_exposed | no | none |
| `jest-spec0234-ex10` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `jest-spec0234-ex11` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0234-ex12` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0234-ex13-error` | authored | not_discriminated | gap | credited | exposed | false_exposed | no | none |
| `jest-spec0234-ex13-globalthis-error` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0234-ex13-typeerror` | authored | not_discriminated | gap | credited | exposed | false_exposed | no | none |
| `jest-spec0234-ex15` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0234-ex16` | authored | not_discriminated | gap | credited | exposed | false_exposed | no | none |
| `jest-spec0234-ex17-default` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0234-ex17-renamed` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `jest-spec0234-ex18` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `jest-spec0234-ex19` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `jest-spec0234-ex20` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0234-ex21` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0234-ex22` | authored | not_discriminated | gap | credited | exposed | false_exposed | no | none |
| `jest-spec0234-ex24` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0234-ex25` | authored | not_discriminated | gap | limited | weakly_exposed | abstained | no | none |
| `jest-spec0234-ex27` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0234-ex30` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `jest-spec0234-ex31` | authored | not_discriminated | gap | credited | exposed | false_exposed | no | none |
| `jest-spec0234-ex32` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `mocha-bonus-boundary-pinned` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `mocha-lineitem-deep-equal` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `mocha-isopen-true-smoke` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `mocha-fee-assert-strictequal` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `mocha-memo-throw-message` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `mocha-reserve-not-equal` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `mocha-limit-async-await` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `mocha-spec8-chai-tobe` | authored | not_discriminated | gap | credited | exposed | false_exposed | no | none |
| `mocha-spec8-chai-to-equal` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `mocha-spec14-chai-throw-string` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `nodetest-tier-boundary-pinned` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `nodetest-invoice-field-deepstrictequal` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `nodetest-bulk-ok-smoke` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `nodetest-currency-throws-regex` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `nodetest-refund-throws-bare` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `nodetest-charge-rejects-message` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `nodetest-latefee-t-assert` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `nodetest-shipping-describe-it` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `nodetest-member-subtest` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `nodetest-spec0234-ex23-await-test` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `nodetest-receipt-legacy-deepequal` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `vitest-orders-order-field-strict-equal` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `vitest-orders-audit-logger-called-with` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `vitest-orders-total-relational-positive` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `vitest-orders-user-rejects-to-throw` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `vitest-orders-tier-each-boundary-pinned` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `vitest-orders-label-snapshot` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `vitest-orders-stock-own-module-mocked` | authored | not_discriminated | gap | limited | no_static_path | abstained | no | none |
| `vitest-orders-spec0234-ex09a-vitest-import` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `vitest-orders-spec0234-ex09c-test-context-expect` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `vitest-orders-spec0234-ex26-spy-namespace` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |

Non-claims:

- Rates describe this corpus only; they are not a population estimate for TypeScript code or for ripr in general.
- Every subject is authored to cover a test-library oracle or reach rule (RIPR-SPEC-0027, RIPR-SPEC-0085, RIPR-SPEC-0234); the rates describe those cells, not real-world TypeScript tests.
- Truth comes from the listed mutants, not from exhaustive mutation; a discriminated label means every listed mutant made the test command fail.
- The harness does not run mutation testing, the TypeScript test runners, or any network access.
