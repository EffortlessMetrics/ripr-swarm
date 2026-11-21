# Python real-mutant calibration receipt (#12901)

Bounded slice over authored in-repo Python subjects. Runtime outcomes come from the
subjects' own pytest suites; ripr verdicts come from `ripr check --diff`. Unknown or
weak verdicts are unscored, never successes.

- ripr: `ripr 0.11.0 (bcea36496818f3f947e9a8dcf31b94f005755527)`; python: `Python 3.13.4`; pytest: `pytest 8.4.0`
- Mutants: 20 across operators arithmetic_operator, comparison_operator, constant_replacement
- Runtime: caught 11, missed 9, unscoreable 0
- Scored claim pairs: 8 (unscored 12)
- Discriminator precision: 1.0 (7/7)
- Missed recall (strict, ungripped of scored missed): 1.0 (1/1)
- Caught recall (strong of scored caught): 1.0 (7/7)
- Flagged-or-unknown share of missed: 1.0 (9/9)

| Mutant | Operator | Runtime | ripr | Grip | Agreement |
| --- | --- | --- | --- | --- | --- |
| pricing-shipping-ge-to-gt | comparison_operator | caught | exposed | strongly_gripped | agree |
| pricing-tier-gold-ge-to-gt | comparison_operator | missed | weakly_exposed | weakly_gripped | unscored |
| pricing-discount-eq-to-ne | comparison_operator | caught | weakly_exposed | weakly_gripped | unscored |
| pricing-discount-rate-10-to-20 | arithmetic_operator | missed | weakly_exposed | weakly_gripped | unscored |
| pricing-tax-rate-8-to-9 | arithmetic_operator | missed | weakly_exposed | weakly_gripped | unscored |
| pricing-total-plus-to-minus | arithmetic_operator | caught | weakly_exposed | weakly_gripped | unscored |
| pricing-parse-le-to-lt | comparison_operator | caught | weakly_exposed | weakly_gripped | unscored |
| pricing-loyalty-divisor-100-to-10 | constant_replacement | missed | weakly_exposed | weakly_gripped | unscored |
| pricing-bulk-ge-to-gt | comparison_operator | caught | exposed | strongly_gripped | agree |
| pricing-bulk-factor-9-to-8 | arithmetic_operator | caught | exposed | strongly_gripped | agree |
| grid-line-total-mul-to-add | arithmetic_operator | caught | exposed | strongly_gripped | agree |
| grid-area-mul-to-add | arithmetic_operator | caught | exposed | strongly_gripped | agree |
| grid-scaled-mul-to-add | arithmetic_operator | caught | weakly_exposed | weakly_gripped | unscored |
| grid-seconds-60-to-61 | constant_replacement | missed | weakly_exposed | weakly_gripped | unscored |
| grid-tip-mul-to-add | arithmetic_operator | caught | exposed | strongly_gripped | agree |
| pricing-silver-rate-5-to-6 | arithmetic_operator | missed | weakly_exposed | weakly_gripped | unscored |
| pricing-receipt-footer-text | constant_replacement | missed | no_static_path | ungripped | agree |
| boundary-is-adult-ge-to-gt | comparison_operator | caught | exposed | strongly_gripped | agree |
| boundary-can-retire-ge-to-gt | comparison_operator | missed | weakly_exposed | weakly_gripped | unscored |
| boundary-overweight-ge-to-gt | comparison_operator | missed | weakly_exposed | weakly_gripped | unscored |
