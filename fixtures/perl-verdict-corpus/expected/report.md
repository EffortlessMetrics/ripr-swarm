# Perl verdict corpus report

Spec: RIPR-SPEC-0238. Corpus version: 2026-10-04.1. Cases: 36.

| Rate | Count | Rate |
| --- | --- | --- |
| False verdicts (all cases) | 13/36 | 0.3611 |
| False actionable (of discriminated) | 9/23 | 0.3913 |
| False exposed (of not fully discriminated) | 4/13 | 0.3077 |
| False silent (of not fully discriminated) | 0/13 | 0.0000 |
| Ideal verdict | 12/36 | 0.3333 |
| Abstained (limited or silent where acceptable) | 11/36 | 0.3056 |
| Findings with a contradiction | 8/36 | 0.2222 |

By fact-packet provenance. An edited packet changes named producer facts to reach a shape a spec defines that the producer does not emit today, so only the producer rates describe what a Perl user gets now.

| Packet | Cases | False verdicts | False actionable | False exposed | False silent | Ideal | Abstained |
| --- | --- | --- | --- | --- | --- | --- | --- |
| edited_producer | 9 | 5/9 | 4/8 | 1/1 | 0/1 | 1/9 | 3/9 |
| producer | 27 | 8/27 | 5/15 | 3/12 | 0/12 | 11/27 | 8/27 |

| Case | Origin | Truth | Ideal | Observed | Classes | Outcome | Changed since labeling | Contradictions |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `perl-testmore-discount-imported-boundary` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `perl-testmore-shipping-smoke-ok` | authored | not_discriminated | gap | gap | reachable_unrevealed | ideal | no | none |
| `perl-testmore-tier-far-from-boundary` | authored | not_discriminated | gap | credited | exposed | false_exposed | no | exposed_without_discriminator |
| `perl-testmore-points-cmp-ok-positive` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `perl-testmore-quote-is-deeply-total` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `perl-testmore-round-cents-like` | authored | partially_discriminated | gap | gap | reachable_unrevealed | ideal | no | none |
| `perl-testmore-clamp-isnt` | authored | not_discriminated | gap | credited | exposed | false_exposed | no | exposed_without_discriminator |
| `perl-testmore-bulk-subtest` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `perl-testmore-tax-table-loop` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `perl-testmore-bundle-helper-sub` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `perl-testmore-cart-method-total` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `perl-testmore-cart-dynamic-dispatch` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `perl-test2-summary-deep-is-count` | authored | discriminated | credited | credited | exposed | ideal | no | exposed_without_discriminator |
| `perl-test2-format-entry-like` | authored | not_discriminated | gap | gap | reachable_unrevealed | ideal | no | none |
| `perl-test2-overdrawn-ok-smoke` | authored | partially_discriminated | gap | gap | reachable_unrevealed | ideal | no | none |
| `perl-test2-withdraw-dies-like-message` | authored | discriminated | credited | gap | reachable_unrevealed | false_actionable | no | none |
| `perl-test2-deposit-lives-only` | authored | not_discriminated | gap | gap | reachable_unrevealed | ideal | no | none |
| `perl-test2-fee-isnt-zero` | authored | not_discriminated | gap | credited | exposed | false_exposed | no | exposed_without_discriminator |
| `perl-test2-net-change-imported` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `perl-test2-close-month-subtest` | authored | discriminated | credited | credited | exposed | ideal | no | exposed_without_discriminator |
| `perl-testexception-withdraw-throws-message` | authored | discriminated | credited | gap | reachable_unrevealed | false_actionable | no | none |
| `perl-testexception-transfer-boundary-pair` | authored | discriminated | credited | gap | reachable_unrevealed | false_actionable | no | none |
| `perl-testexception-close-out-dies-ok` | authored | not_discriminated | gap | gap | reachable_unrevealed | ideal | no | none |
| `perl-testexception-interest-lives-ok` | authored | not_discriminated | gap | gap | reachable_unrevealed | ideal | no | none |
| `perl-testexception-freeze-class` | authored | discriminated | credited | gap | reachable_unrevealed | false_actionable | no | none |
| `perl-testexception-lives-and-is` | authored | discriminated | credited | credited | exposed | ideal | no | exposed_without_discriminator |
| `perl-testmore-can-ok-only` | authored | not_discriminated | gap | limited | no_static_path | abstained | no | none |
| `perl-0235-ex01-sink-unconfirmed` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `perl-0235-ex12-static-unknown-hint` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `perl-0235-ex19-sink-whitespace` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `perl-0235-ex23-missing-test-runner` | authored | discriminated | credited | credited | exposed | ideal | no | exposed_without_discriminator |
| `perl-0235-ex24-framework-indirection` | authored | discriminated | credited | limited | weakly_exposed | abstained | no | none |
| `perl-0235-ex25-dynamic-dispatch` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `perl-0235-ex28-cross-test-oracle` | authored | not_discriminated | gap | credited | exposed | false_exposed | no | exposed_without_discriminator |
| `perl-0235-ex13-method-receiver` | authored | discriminated | credited | gap | reachable_unrevealed | false_actionable | no | none |
| `perl-0235-ex14-helper-call` | authored | discriminated | credited | gap | reachable_unrevealed | false_actionable | no | none |

Non-claims:

- Rates describe this corpus only; they are not a population estimate for Perl code or for ripr in general.
- Every subject is authored to fill a cell of ripr's Perl oracle and reach rules; its rates are not real-world rates.
- ripr does not parse Perl. Every verdict here is ripr's reading of a committed fact packet, so a wrong verdict can come from the producer's facts or from ripr's rules; the case reasoning says which.
- edited_producer cases measure ripr's rules on facts the pinned producer does not emit; they are not evidence about the producer.
- Truth comes from the listed hand-written mutants, not from exhaustive mutation; a discriminated label means every listed mutant made prove fail.
- The harness does not run Perl, prove, the fact producer, mutation testing, or any network access.
