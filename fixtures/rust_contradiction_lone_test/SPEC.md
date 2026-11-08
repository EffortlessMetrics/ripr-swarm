# Fixture: rust_contradiction_lone_test

Spec: RIPR-SPEC-0108
Honesty contract: RIPR-SPEC-0108. Issues: #10980, #11012.

## Given

A changed owner (`is_adult`) whose only related test carries mutually
contradictory boundary assertions (`assert!(is_adult(18));` then
`assert!(!is_adult(18));`) that fail on both diff versions.

## When

The owner changes from `age > 18` to `age >= 18`.

## Then

The return-value finding reads `infection_unknown` with the
activation-credit-withheld disclosure (#10980/#11012): the baseline-failing
test's credit is withheld, so the delta never reads promoted. This corpus
entry pins the landed lone-shape control that #12412's composition fix must
preserve.

## Must Not

- Promote the finding to `exposed` or `weakly_exposed` off the contradicted
  test's assertion rows.
- Report the fixture clean; the delta is analyzed and disclosed as unknown.
