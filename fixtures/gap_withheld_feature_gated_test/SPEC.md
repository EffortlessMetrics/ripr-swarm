# Fixture: gap_withheld_feature_gated_test

Spec: RIPR-SPEC-0240

## Given

The return of `weight(input)` changes from `input * 2` to `input * 3`. The
only related test asserts `assert_eq!(weight(4), 12)` and carries
`#[cfg(feature = "std")]`, a default feature. ripr does not evaluate Cargo
features, so it refuses the assertion for a limit of its own reading.

## When

`cargo xtask fixtures gap_withheld_feature_gated_test` runs the public
analyzer.

## Then

The return-value finding reads `static_unknown` with `static_limit_kind`
`rust_assertion_context_unresolved` and stop reason
`gap_evidence_unresolved`. Its next step names `checks_weight`.

## Must Not

- Report `reachable_unrevealed`: the test runs under default features and
  the assertion catches the change.
- Report `exposed`: ripr did not credit the assertion.
- Build a repair route or missing discriminator from the withheld gap.
