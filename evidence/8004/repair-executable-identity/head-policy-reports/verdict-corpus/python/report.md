# Python verdict corpus report

Spec: RIPR-SPEC-0245. Cases: 69.

| Rate | Count | Rate |
| --- | --- | --- |
| False verdicts (all cases) | 0/69 | 0.0000 |
| False actionable (of discriminated) | 0/35 | 0.0000 |
| False exposed (of not fully discriminated) | 0/34 | 0.0000 |
| False silent (of not fully discriminated) | 0/34 | 0.0000 |
| Ideal verdict | 54/69 | 0.7826 |
| Abstained (limited or silent where acceptable) | 15/69 | 0.2174 |
| Findings with a contradiction | 0/69 | 0.0000 |

| Case | Origin | Truth | Ideal | Observed | Classes | Outcome | Changed since labeling | Contradictions |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `py-blocks-account-clamp-one-line` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-blocks-account-clamp-second-line` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `py-blocks-wallet-clamp-second-line` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `py-fixtures-deposit` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `py-fixtures-fee-boundary` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `py-fixtures-greeting-default` | authored | not_discriminated | gap | limited | static_unknown | abstained | no | none |
| `py-fixtures-report-file` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `py-fixtures-warning-stderr` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `py-hypothesis-absolute-reference` | authored | not_discriminated | gap | limited | static_unknown | abstained | no | none |
| `py-hypothesis-clamp-invariant` | authored | not_discriminated | gap | limited | static_unknown | abstained | no | none |
| `py-hypothesis-label-no-crash` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `py-hypothesis-mean-bounds` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `py-hypothesis-passing-threshold` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `py-pytest-bulk-default` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-pytest-footer-unreferenced` | authored | not_discriminated | gap | limited | no_static_path | abstained | no | none |
| `py-pytest-gold-discount-rate` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-pytest-gold-threshold` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-pytest-label-case` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-pytest-points-divisor` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-pytest-quantity-guard` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-pytest-quote-shipping-term` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `py-pytest-receipt-format` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `py-pytest-shipping-threshold` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `py-pytest-silver-parametrized` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `py-pytest-tax-self-computed` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `py-spec0233-ex01-exact` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `py-spec0233-ex02-not-equal` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-spec0233-ex03-assert-not-equal` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-spec0233-ex04-isinstance` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-spec0233-ex05-and-chain` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `py-spec0233-ex06-self-compare` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `py-spec0233-ex07-approx` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `py-spec0233-ex08-almost-equal` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `py-spec0233-ex09-mock-arg` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `py-spec0233-ex09-mock-sign` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-spec0233-ex10-numpy-helper` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `py-spec0233-ex11-fluent-helper` | authored | discriminated | credited | limited | static_unknown | abstained | yes | none |
| `py-spec0233-ex12-exact-then-other` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `py-spec0233-ex13-other-then-exact` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `py-spec0233-ex14-raises-match` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `py-spec0233-ex15-raises-broad` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-spec0233-ex16-value-only` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-spec0233-ex17-raises-match-and-value` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `py-spec0233-ex18-split-tests` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `py-spec0233-ex19-match-anything` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-spec0233-ex20-raises-regex` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `py-spec0233-ex21-exc-value` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `py-spec0233-ex22-dict-field` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-spec0233-ex22-dict-whole` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `py-spec0233-ex23-list-index` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-spec0233-ex24-method-bound` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `py-spec0233-ex24-method-orthogonal` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `py-spec0233-ex25-rival-import` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-spec0233-ex26-call-named-type` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `py-spec0233-ex27-module-imports-os` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `py-spec0233-ex28-lambda-in-string` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `py-spec0233-ex29-client-patch` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `py-spec0233-ex30-property` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `py-spec0233-ex31-fstring-length` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `py-spec0233-ex32-split-dict` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `py-spec0233-ex33-spaced-getattr` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `py-spec0233-ex34-exc-length` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-spec0233-ex34-exc-not-none` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-unittest-can-withdraw` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-unittest-deposit-add` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `py-unittest-fee-waiver-mixin` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `py-unittest-overdraft-boundary` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-unittest-owner-strip` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `py-unittest-transfer-notify` | authored | discriminated | credited | credited | exposed | ideal | yes | none |

## Spec example coverage

Numbered acceptance examples of in-scope specs that at least one case cites in `spec_examples` (fixtures/python-verdict-corpus/spec-coverage.toml). Covered 40/40 (1.0000); floor 40. Waived 0 and accounted (covered plus waived) 40/40. In-scope specs 1, out-of-scope specs 84.

Unmeasured in-scope specs (prose acceptance examples, not counted): RIPR-SPEC-0004, RIPR-SPEC-0028, RIPR-SPEC-0035, RIPR-SPEC-0201.

| Spec | In-scope examples | Covered | Waived | Uncovered |
| --- | --- | --- | --- | --- |
| RIPR-SPEC-0238 | 40 | 40 | 0 | none |

Non-claims:

- Rates describe this corpus only; they are not a population estimate for Python code or for ripr in general.
- Every subject is authored to cover a test-library oracle or reach rule; the rates describe those cells, not real-world Python tests.
- Truth comes from the listed mutants, not from exhaustive mutation; a discriminated label means every listed mutant made the test command fail.
- The harness does not run mutation testing, the Python test runners, or any network access.
