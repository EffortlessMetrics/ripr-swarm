# Python verdict corpus report

Spec: RIPR-SPEC-0238. Corpus version: 2026-10-04.1. Cases: 61.

| Rate | Count | Rate |
| --- | --- | --- |
| False verdicts (all cases) | 21/61 | 0.3443 |
| False actionable (of discriminated) | 15/33 | 0.4545 |
| False exposed (of not fully discriminated) | 6/28 | 0.2143 |
| False silent (of not fully discriminated) | 0/28 | 0.0000 |
| Ideal verdict | 25/61 | 0.4098 |
| Abstained (limited or silent where acceptable) | 15/61 | 0.2459 |
| Findings with a contradiction | 0/61 | 0.0000 |

| Case | Origin | Truth | Ideal | Observed | Classes | Outcome | Changed since labeling | Contradictions |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `py-pytest-shipping-threshold` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `py-pytest-gold-threshold` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-pytest-silver-parametrized` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `py-pytest-gold-discount-rate` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-pytest-tax-self-computed` | authored | not_discriminated | gap | credited | exposed | false_exposed | no | none |
| `py-pytest-quantity-guard` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-pytest-points-divisor` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-pytest-label-case` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-pytest-bulk-default` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-pytest-quote-shipping-term` | authored | not_discriminated | gap | credited | exposed | false_exposed | no | none |
| `py-pytest-receipt-format` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `py-pytest-footer-unreferenced` | authored | not_discriminated | gap | limited | no_static_path | abstained | no | none |
| `py-unittest-deposit-add` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `py-unittest-owner-strip` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-unittest-can-withdraw` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-unittest-overdraft-boundary` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-unittest-transfer-notify` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `py-unittest-fee-waiver-mixin` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `py-spec0233-ex01-exact` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `py-spec0233-ex02-not-equal` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-spec0233-ex03-assert-not-equal` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-spec0233-ex04-isinstance` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-spec0233-ex05-and-chain` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `py-spec0233-ex06-self-compare` | authored | not_discriminated | gap | credited | exposed | false_exposed | no | none |
| `py-spec0233-ex07-approx` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `py-spec0233-ex08-almost-equal` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `py-spec0233-ex11-fluent-helper` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `py-spec0233-ex12-exact-then-other` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `py-spec0233-ex13-other-then-exact` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `py-spec0233-ex14-raises-match` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `py-spec0233-ex15-raises-broad` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-spec0233-ex16-value-only` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-spec0233-ex17-raises-match-and-value` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `py-spec0233-ex18-split-tests` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `py-spec0233-ex19-match-anything` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-spec0233-ex20-raises-regex` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `py-spec0233-ex21-exc-value` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `py-spec0233-ex22-dict-field` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-spec0233-ex22-dict-whole` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `py-spec0233-ex23-list-index` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-spec0233-ex24-method-orthogonal` | authored | not_discriminated | gap | credited | exposed | false_exposed | no | none |
| `py-spec0233-ex24-method-bound` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `py-spec0233-ex25-rival-import` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-spec0233-ex26-call-named-type` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `py-spec0233-ex27-module-imports-os` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `py-spec0233-ex28-lambda-in-string` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `py-spec0233-ex29-client-patch` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `py-spec0233-ex30-property` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `py-spec0233-ex31-fstring-length` | authored | not_discriminated | gap | credited | exposed | false_exposed | no | none |
| `py-spec0233-ex32-split-dict` | authored | not_discriminated | gap | credited | exposed | false_exposed | no | none |
| `py-spec0233-ex33-spaced-getattr` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `py-hypothesis-clamp-invariant` | authored | not_discriminated | gap | limited | static_unknown | abstained | no | none |
| `py-hypothesis-absolute-reference` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `py-hypothesis-mean-bounds` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `py-hypothesis-passing-threshold` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `py-hypothesis-label-no-crash` | authored | not_discriminated | gap | limited | static_unknown | abstained | no | none |
| `py-fixtures-deposit` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `py-fixtures-fee-boundary` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `py-fixtures-report-file` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `py-fixtures-greeting-default` | authored | not_discriminated | gap | limited | static_unknown | abstained | no | none |
| `py-fixtures-warning-stderr` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |

Non-claims:

- Rates describe this corpus only; they are not a population estimate for Python code or for ripr in general.
- Every subject is authored to cover a test-library oracle or reach rule; the rates describe those cells, not real-world Python tests.
- Truth comes from the listed mutants, not from exhaustive mutation; a discriminated label means every listed mutant made the test command fail.
- The harness does not run mutation testing, the Python test runners, or any network access.
