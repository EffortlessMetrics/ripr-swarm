# Rust verdict corpus report

Spec: RIPR-SPEC-0219. Cases: 5.

| Rate | Count | Rate |
| --- | --- | --- |
| False verdicts (all cases) | 5/5 | 1.0000 |
| False actionable (of discriminated) | 5/5 | 1.0000 |
| False exposed (of not fully discriminated) | 0/0 | n/a |
| False silent (of not fully discriminated) | 0/0 | n/a |
| Ideal verdict | 0/5 | 0.0000 |
| Abstained (limited or silent where acceptable) | 0/5 | 0.0000 |
| Findings with a contradiction | 0/5 | 0.0000 |

By subject origin. Authored cases are written to fill cells the upstream cases leave empty, so only the upstream rates describe real-world tests.

| Origin | Cases | False verdicts | False actionable | False exposed | False silent | Ideal | Abstained |
| --- | --- | --- | --- | --- | --- | --- | --- |
| authored | 1 | 1/1 | 1/1 | 0/0 | 0/0 | 0/1 | 0/1 |
| upstream | 4 | 4/4 | 4/4 | 0/0 | 0/0 | 0/4 | 0/4 |

| Case | Origin | Truth | Ideal | Observed | Classes | Outcome | Changed since labeling | Contradictions |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `bytesize-as-kib-div` | upstream | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `bytesize-as-mb-div` | upstream | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `lib-rstest-fixture-injected-receiver` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `serde-derive-rename-field-upper` | upstream | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `serde-derive-rename-variant-lower` | upstream | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |

## Spec example coverage

Numbered acceptance examples of in-scope specs that at least one case cites in `spec_examples` (fixtures/rust-verdict-corpus/spec-coverage.toml). Covered 119/124 (0.9597); floor 119. Waived 15 and accounted (covered plus waived) 134/139. In-scope specs 17, out-of-scope specs 68.

Unmeasured in-scope specs (prose acceptance examples, not counted): RIPR-SPEC-0001, RIPR-SPEC-0002, RIPR-SPEC-0004, RIPR-SPEC-0005, RIPR-SPEC-0035, RIPR-SPEC-0040, RIPR-SPEC-0048, RIPR-SPEC-0093, RIPR-SPEC-0094, RIPR-SPEC-0096, RIPR-SPEC-0106, RIPR-SPEC-0107, RIPR-SPEC-0108, RIPR-SPEC-0150, RIPR-SPEC-0153, RIPR-SPEC-0154, RIPR-SPEC-0157, RIPR-SPEC-0158, RIPR-SPEC-0159, RIPR-SPEC-0163, RIPR-SPEC-0164, RIPR-SPEC-0165, RIPR-SPEC-0168, RIPR-SPEC-0169, RIPR-SPEC-0171, RIPR-SPEC-0172, RIPR-SPEC-0175, RIPR-SPEC-0186, RIPR-SPEC-0197, RIPR-SPEC-0219.

| Spec | In-scope examples | Covered | Waived | Uncovered |
| --- | --- | --- | --- | --- |
| RIPR-SPEC-0114 | 6 | 5 | 1 | none |
| RIPR-SPEC-0115 | 7 | 3 | 4 | none |
| RIPR-SPEC-0117 | 7 | 7 | 0 | none |
| RIPR-SPEC-0118 | 4 | 4 | 0 | none |
| RIPR-SPEC-0119 | 2 | 2 | 0 | none |
| RIPR-SPEC-0120 | 3 | 3 | 0 | none |
| RIPR-SPEC-0125 | 3 | 1 | 2 | none |
| RIPR-SPEC-0224 | 2 | 1 | 1 | none |
| RIPR-SPEC-0225 | 16 | 16 | 0 | none |
| RIPR-SPEC-0226 | 4 | 4 | 0 | none |
| RIPR-SPEC-0227 | 13 | 13 | 0 | none |
| RIPR-SPEC-0228 | 15 | 13 | 2 | none |
| RIPR-SPEC-0229 | 10 | 9 | 1 | none |
| RIPR-SPEC-0231 | 31 | 31 | 0 | none |
| RIPR-SPEC-0233 | 4 | 0 | 4 | none |
| RIPR-SPEC-0263 | 6 | 1 | 0 | 2, 3, 4, 5, 6 |
| RIPR-SPEC-0265 | 6 | 6 | 0 | none |

Non-claims:

- Rates describe this corpus only; they are not a population estimate for Rust code or for ripr in general.
- Authored cases were written to fill verdict, probe-family, test-shape and test-library oracle cells the upstream cases leave empty; their rates are reported separately under by_origin and are not real-world rates.
- Truth comes from the listed mutants, not from exhaustive mutation; a discriminated label means every listed mutant made the test command fail.
- The harness runs ripr on retained upstream excerpts and on whole authored crates; excerpt findings matched the full pinned checkout at labeling time and must be re-checked when a verdict changes.
- The harness does not run mutation testing, cargo test, or any network access.
