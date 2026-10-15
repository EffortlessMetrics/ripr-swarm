# TypeScript verdict corpus report

Spec: RIPR-SPEC-0245. Cases: 169.

| Rate | Count | Rate |
| --- | --- | --- |
| False verdicts (all cases) | 7/169 | 0.0414 |
| False actionable (of discriminated) | 7/101 | 0.0693 |
| False exposed (of not fully discriminated) | 0/68 | 0.0000 |
| False silent (of not fully discriminated) | 0/68 | 0.0000 |
| Ideal verdict | 150/169 | 0.8876 |
| Abstained (limited or silent where acceptable) | 12/169 | 0.0710 |
| Findings with a contradiction | 0/169 | 0.0000 |

| Case | Origin | Truth | Ideal | Observed | Classes | Outcome | Changed since labeling | Contradictions |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `jest-age-not-tobe` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-cents-custom-matcher` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `jest-coupon-tothrow-broad` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-discount-boundary-pinned` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-mined-default-export-object` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `jest-mined-private-impl-dispatcher` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `jest-mined-suite-factory` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `jest-mined-test-fn-reference` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `jest-mined-tobe-identity` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-mined-tobeinstanceof-bound-local` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-mined-tobeinstanceof-error-twice` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-mined-tobeinstanceof` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `jest-mined-toequal-copy-not-identity` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `jest-mined-tomatchobject-bound-local` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-mined-tomatchobject-field` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `jest-mined-tomatchobject-name-only` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-mock-fn-spread-object` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `jest-notify-called-with` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `jest-order-assert-in-then` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `jest-price-resolves-tobe` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-quantity-tothrow-message` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-receipt-inline-snapshot` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `jest-shipping-boundary-unpinned` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-sku-method-chain` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0234-ex01` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0234-ex02` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0234-ex03` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `jest-spec0234-ex04` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0234-ex05` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0234-ex06` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0234-ex07` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `jest-spec0234-ex10` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `jest-spec0234-ex11` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0234-ex12` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0234-ex13-error` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `jest-spec0234-ex13-globalthis-error` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0234-ex13-typeerror` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `jest-spec0234-ex15` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0234-ex16` | authored | not_discriminated | gap | limited | no_static_path | abstained | yes | none |
| `jest-spec0234-ex17-default` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0234-ex17-renamed` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `jest-spec0234-ex18` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `jest-spec0234-ex19` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `jest-spec0234-ex20` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0234-ex21` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0234-ex22` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `jest-spec0234-ex24` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0234-ex25` | authored | not_discriminated | gap | limited | weakly_exposed | abstained | no | none |
| `jest-spec0234-ex27` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0234-ex28` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0234-ex29` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `jest-spec0234-ex30` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `jest-spec0234-ex31` | authored | not_discriminated | gap | limited | static_unknown | abstained | yes | none |
| `jest-spec0234-ex32` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0243-ex30-factory-db` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `jest-spec0243-ex33-two-files-one-mocked` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0246-ex03-spread-drops-field` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `jest-spec0246-ex04-foreach-two-literals` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0246-ex05-foreach-variable-element` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `jest-spec0246-ex07-then-rejection-handler` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `jest-spec0246-ex09-async-foreach` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `jest-spec0246-ex10-member-pin-then-toequal` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0247-ex03-second-send` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0247-ex05-spyon-send` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `jest-spec0248-drop-fee-rebound` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0248-drop-fee-second-input` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0248-drop-fee-sign-only` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0248-drop-fee-zero-only` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0248-drop-fee` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0248-drop-flat` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0248-drop-literal-zero` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0248-multiplied-term` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0248-throw-message-term` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `jest-spec0249-ex03-subclass-of-pinned` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0249-ex05-wrapped-pin` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `jest-spec0249-ex06-fall-through-if` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0249-ex07-one-line-if` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0249-ex08-test-local-class` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0249-ex09-message-change` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0250-ex04-normalized-result` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `jest-spec0250-ex05-non-literal-expected` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `jest-spec0250-ex06-spread-after-field` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0251-ex02-default-binds-other` | authored | not_discriminated | gap | limited | no_static_path | abstained | no | none |
| `jest-spec0251-ex03-body-shadows-default` | authored | not_discriminated | gap | limited | no_static_path | abstained | no | none |
| `jest-spec0251-ex04-strong-pin-on-other-default` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0251-ex05-destructured-local` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0251-ex06-one-hop-local` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0251-ex07-default-drop-fee-nonzero` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0251-ex07-default-drop-fee-zero` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0256-ex02-two-entry-object` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0256-ex03-after-braced-if` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0256-ex04-braceless-if` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `jest-spec0256-ex05-direct-mock-call` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0256-ex06-called-times` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0258-ex02-describe-title` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-spec0258-ex03-no-snapshot-file` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0258-ex04-hint` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `jest-spec0258-ex05-update-script` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-spec0258-ex06-spaced-call` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `jest-stock-truthy-only` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `jest-tax-module-mocked` | authored | not_discriminated | gap | limited | no_static_path | abstained | no | none |
| `jest-tier-test-each-table` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `mocha-bonus-boundary-pinned` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `mocha-fee-assert-strictequal` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `mocha-isopen-true-smoke` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `mocha-label-throw-second-replaced-line` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `mocha-label-throw-uneven-longer-block` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `mocha-label-throw-uneven-shorter-block` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `mocha-limit-async-await` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `mocha-lineitem-deep-equal` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `mocha-memo-throw-message` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `mocha-reserve-not-equal` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `mocha-spec14-chai-throw-string` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `mocha-spec8-chai-to-equal` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `mocha-spec8-chai-tobe` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `mocha-title-throw-second-replaced-line` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `mocha-title-throw-uneven-shorter-block` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `nodetest-bulk-ok-smoke` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `nodetest-charge-rejects-message` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `nodetest-currency-throws-regex` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `nodetest-invoice-field-deepstrictequal` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `nodetest-latefee-t-assert` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `nodetest-legacy-equal-literal-one` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `nodetest-member-subtest` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `nodetest-receipt-legacy-deepequal` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `nodetest-refund-throws-bare` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `nodetest-shipping-describe-it` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `nodetest-spec0234-ex23-await-test` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `nodetest-spec0243-ex34-throws-anchored-regex` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `nodetest-spec0243-ex35-rejects-message-changed` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `nodetest-subtest-t2-assert-strictequal` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `nodetest-t-assert-deepstrictequal` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `nodetest-t-assert-equal-variable` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `nodetest-t-assert-throws-regex-message` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `nodetest-t-bracket-assert` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `nodetest-tier-boundary-pinned` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `ts-nodetest-audit-log` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `ts-nodetest-charge-rate` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `ts-nodetest-footer-unasserted` | authored | not_discriminated | gap | limited | no_static_path | abstained | no | none |
| `ts-nodetest-gold-discount-rate` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `ts-nodetest-gold-threshold` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `ts-nodetest-quantity-guard` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `ts-nodetest-quote-shipping-term` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `ts-nodetest-shipping-threshold` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `ts-nodetest-silver-legacy-assert` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `ts-nodetest-tax-self-computed` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `ts-vitest-add-line-message` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `ts-vitest-bulk-threshold` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `ts-vitest-checkout-notify` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `ts-vitest-is-empty` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `ts-vitest-line-total-reexport` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `ts-vitest-remove-line-message` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `ts-vitest-summary-snapshot` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `ts-vitest-unit-price-table-other-key` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `ts-vitest-unit-price-table` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `vitest-orders-audit-logger-called-with` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `vitest-orders-label-snapshot` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `vitest-orders-order-field-strict-equal` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `vitest-orders-spec0234-ex09a-vitest-import` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `vitest-orders-spec0234-ex09c-test-context-expect` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `vitest-orders-spec0234-ex26-spy-namespace` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `vitest-orders-stock-own-module-mocked` | authored | not_discriminated | gap | limited | no_static_path | abstained | no | none |
| `vitest-orders-tier-each-boundary-pinned` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `vitest-orders-total-relational-positive` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `vitest-orders-user-rejects-to-throw` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `vitest-spec0254-ex02-awaited-local` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `vitest-spec0254-ex03-unchanged-key` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `vitest-spec0254-ex04-value-to-null` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `vitest-spec0254-ex05-region-key` | authored | discriminated | credited | credited | exposed | ideal | no | none |

## Spec example coverage

Numbered acceptance examples of in-scope specs that at least one case cites in `spec_examples` (fixtures/typescript-verdict-corpus/spec-coverage.toml). Covered 112/120 (0.9333); floor 112. Waived 0 and accounted (covered plus waived) 112/120. In-scope specs 12, out-of-scope specs 73.

Unmeasured in-scope specs (prose acceptance examples, not counted): RIPR-SPEC-0027, RIPR-SPEC-0095, RIPR-SPEC-0097, RIPR-SPEC-0098.

| Spec | In-scope examples | Covered | Waived | Uncovered |
| --- | --- | --- | --- | --- |
| RIPR-SPEC-0243 | 35 | 35 | 0 | none |
| RIPR-SPEC-0246 | 14 | 10 | 0 | 11, 12, 13, 14 |
| RIPR-SPEC-0247 | 11 | 11 | 0 | none |
| RIPR-SPEC-0248 | 10 | 10 | 0 | none |
| RIPR-SPEC-0249 | 9 | 9 | 0 | none |
| RIPR-SPEC-0250 | 6 | 6 | 0 | none |
| RIPR-SPEC-0251 | 7 | 7 | 0 | none |
| RIPR-SPEC-0252 | 7 | 6 | 0 | 6 |
| RIPR-SPEC-0254 | 5 | 5 | 0 | none |
| RIPR-SPEC-0256 | 6 | 6 | 0 | none |
| RIPR-SPEC-0258 | 6 | 6 | 0 | none |
| RIPR-SPEC-0262 | 4 | 1 | 0 | 2, 3, 4 |

Non-claims:

- Rates describe this corpus only; they are not a population estimate for TypeScript code or for ripr in general.
- Every subject is authored to cover a test-library oracle or reach rule (RIPR-SPEC-0027, RIPR-SPEC-0085, RIPR-SPEC-0243); the rates describe those cells, not real-world TypeScript tests.
- Truth comes from the listed mutants, not from exhaustive mutation; a discriminated label means every listed mutant made the test command fail.
- The harness does not run mutation testing, the TypeScript test runners, or any network access.
