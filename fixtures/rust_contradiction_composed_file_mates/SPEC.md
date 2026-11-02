# Fixture: rust_contradiction_composed_file_mates

Spec: RIPR-SPEC-0108
Honesty contract: RIPR-SPEC-0108. Issue: #12412.

## Given

A changed owner (`is_adult`) whose whole-test contradictory assertions
(`assert!(is_adult(18)); assert!(!is_adult(18));`) fail on both diff
versions, beside two unrelated same-file tests that never call the owner
(they reach only `clamp_to_range` and `discounted_total`).

## When

The owner changes from `age > 18` to `age >= 18`.

## Then

Withholding survives composition: the return-value finding stays
`infection_unknown` with the activation-credit-withheld disclosure, because
no credited related test establishes reach to the owner. The file-mates
carry proximity credit only, so a credited-count gate would wrongly let the
infection stage's default yes promote this to `exposed` on the strength of
the contradicted test's own oracle rows.

## Must Not

- Promote the return-value finding to `exposed` because unrelated
  file-mates keep the credited activation set non-empty.
- Credit the composed shape above the lone contradictory-test shape, which
  reads `infection_unknown` on the same delta.
- Read the withheld test's assertion rows as a strong oracle for
  activation.
