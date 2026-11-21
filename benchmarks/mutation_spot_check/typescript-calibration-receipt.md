# TypeScript real-mutant calibration receipt (#12901)

Bounded slice over authored in-repo TypeScript subjects. Runtime outcomes come from the
subjects' own node:test suites; ripr verdicts come from `ripr check --diff`. Unknown or
weak verdicts are unscored, never successes.

- ripr: `ripr 0.11.0 (29d5ac7ab852fbab0a6cb1765a17476457a0b6ab)`; node: `v26.5.0`
- Mutants: 19 across operators arithmetic_operator, comparison_operator, constant_replacement
- Runtime: caught 11, missed 8, unscoreable 0
- Scored claim pairs: 4 (unscored 15)
- Discriminator precision: 1.0 (3/3)
- Missed recall (strict, ungripped of scored missed): 1.0 (1/1)
- Caught recall (strong of scored caught): 1.0 (3/3)
- Flagged-or-unknown share of missed: 0.625 (5/8)

| Mutant | Operator | Runtime | ripr | Grip | Agreement |
| --- | --- | --- | --- | --- | --- |
| pricing-shipping-ge-to-gt | comparison_operator | caught | exposed | strongly_gripped | agree |
| pricing-tier-gold-ge-to-gt | comparison_operator | missed | weakly_exposed | weakly_gripped | unscored |
| pricing-tier-silver-ge-to-gt | comparison_operator | caught | exposed | strongly_gripped | agree |
| pricing-discount-gold-10-to-20 | constant_replacement | missed | weakly_exposed | weakly_gripped | unscored |
| pricing-discount-silver-5-to-6 | constant_replacement | missed | weakly_exposed | weakly_gripped | unscored |
| pricing-tax-8-to-9 | constant_replacement | missed | weakly_exposed | weakly_gripped | unscored |
| pricing-total-plus-to-minus | arithmetic_operator | caught | exposed | strongly_gripped | agree |
| pricing-parse-le-to-lt | comparison_operator | caught | weakly_exposed | weakly_gripped | unscored |
| pricing-receipt-footer-text | constant_replacement | missed | no_static_path | ungripped | agree |
| grid-line-total-mul-to-add | arithmetic_operator | caught | analysis_incomplete | - | unscored |
| grid-area-mul-to-add | arithmetic_operator | caught | analysis_incomplete | - | unscored |
| grid-scaled-mul-to-add | arithmetic_operator | caught | analysis_incomplete | - | unscored |
| grid-seconds-60-to-61 | constant_replacement | missed | analysis_incomplete | - | unscored |
| grid-tip-mul-to-add | arithmetic_operator | caught | analysis_incomplete | - | unscored |
| boundary-is-adult-ge-to-gt | comparison_operator | caught | analysis_incomplete | - | unscored |
| boundary-free-shipping-ge-to-gt | comparison_operator | caught | analysis_incomplete | - | unscored |
| boundary-can-retire-ge-to-gt | comparison_operator | missed | analysis_incomplete | - | unscored |
| boundary-passes-ge-to-gt | comparison_operator | caught | analysis_incomplete | - | unscored |
| boundary-overweight-ge-to-gt | comparison_operator | missed | analysis_incomplete | - | unscored |
