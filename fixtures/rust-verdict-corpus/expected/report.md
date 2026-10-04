# Rust verdict corpus report

Spec: RIPR-SPEC-0219. Corpus version: 2026-10-04.1. Cases: 23.

| Rate | Count | Rate |
| --- | --- | --- |
| False verdicts (all cases) | 6/23 | 0.2609 |
| False actionable (of discriminated) | 6/15 | 0.4000 |
| False exposed (of not fully discriminated) | 0/8 | 0.0000 |
| False silent (of not fully discriminated) | 0/8 | 0.0000 |
| Ideal verdict | 4/23 | 0.1739 |
| Abstained (limited or silent where acceptable) | 13/23 | 0.5652 |
| Findings with a contradiction | 2/29 | 0.0690 |

| Case | Truth | Ideal | Observed | Classes | Outcome | Changed since labeling | Contradictions |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `serde-format-u8-hundreds` | discriminated | credited | gap | reachable_unrevealed | false_actionable | no | none |
| `serde-format-u8-tens` | discriminated | credited | limited | propagation_unknown | abstained | no | none |
| `semver-caret-minor-ge` | discriminated | credited | limited | no_static_path | abstained | no | none |
| `semver-tilde-pre-ge` | discriminated | credited | limited | infection_unknown, propagation_unknown | abstained | no | none |
| `semver-less-pre` | discriminated | credited | limited | infection_unknown, propagation_unknown | abstained | no | none |
| `semver-greater-patch` | discriminated | credited | limited | no_static_path | abstained | no | none |
| `semver-digit-upper` | partially_discriminated | gap | limited | infection_unknown | abstained | no | none |
| `semver-max-comparators` | partially_discriminated | gap | limited | no_static_path | abstained | no | none |
| `semver-caret-zero-minor` | partially_discriminated | gap | limited | no_static_path | abstained | no | none |
| `hex-from-hex-odd-variant` | discriminated | credited | credited | exposed | ideal | yes | none |
| `hex-decode-slice-odd` | discriminated | credited | gap | weakly_exposed | false_actionable | yes | none |
| `hex-decode-slice-length` | partially_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `hex-encode-slice-length` | partially_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `itoa-two-digit-tail` | not_discriminated | gap | limited | no_static_path | abstained | no | none |
| `regex-syntax-word-byte` | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `regex-syntax-max-scalar-two-byte` | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `serde-derive-rename-variant-lower` | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `serde-derive-rename-field-upper` | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `semver-leading-zero` | discriminated | credited | limited | no_static_path | abstained | no | none |
| `semver-op-greater-eq` | discriminated | credited | gap | reachable_unrevealed | false_actionable | no | reach_yes_without_related_tests |
| `itoa-four-digit-loop` | not_discriminated | gap | limited | no_static_path | abstained | no | none |
| `semver-digit-upper-first-run` | discriminated | credited | limited | infection_unknown | abstained | no | none |
| `bytesize-format-unit-first-run` | discriminated | credited | limited | infection_unknown | abstained | no | none |

Non-claims:

- Rates describe this corpus only; they are not a population estimate for Rust code or for ripr in general.
- Truth comes from the listed mutants, not from exhaustive mutation; a discriminated label means every listed mutant made the test command fail.
- The harness runs ripr on retained upstream excerpts; excerpt findings matched the full pinned checkout at labeling time and must be re-checked when a verdict changes.
- The harness does not run mutation testing, cargo test, or any network access.
