# Fixture: gap_kept_ignored_test

Spec: RIPR-SPEC-0240

## Given

The return of `weight(input)` changes from `input * 2` to `input * 3`. The
only related test asserts `assert_eq!(weight(4), 12)` and carries
`#[ignore]`, so it does not run by default. This is the discriminating
control for `gap_withheld_feature_gated_test`.

## When

`cargo xtask fixtures gap_kept_ignored_test` runs the public analyzer.

## Then

The return-value finding stays `reachable_unrevealed`: a refusal on an
attribute that keeps the test from running is evidence of a gap, not an
analyzer limit.

## Must Not

- Withhold the gap as `static_unknown`.
- Report `exposed`.
