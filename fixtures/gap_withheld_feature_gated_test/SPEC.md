# Fixture: gap_withheld_feature_gated_test

Spec: RIPR-SPEC-0240

## Given

The return of `weight(input)` changes from `input * 2` to `input * 3`. The
only related test asserts `assert_eq!(weight(4), 12)` and carries
`#[cfg(feature = "std")]`, a feature the manifest declares but leaves out of
`default`. Which features a run enables beyond the defaults is not known
statically, so ripr refuses the assertion for a limit of its own reading. The
same gate on a default feature is resolved and the assertion credited (#8780,
`crates/ripr/tests/default_feature_test_gate.rs`).

## When

`cargo xtask fixtures gap_withheld_feature_gated_test` runs the public
analyzer.

## Then

The return-value finding reads `static_unknown` with `static_limit_kind`
`rust_assertion_context_unresolved` and stop reason
`gap_evidence_unresolved`. Its next step names `checks_weight`.

## Must Not

- Report `reachable_unrevealed`: a run with `--features std` compiles the
  test and the assertion catches the change.
- Report `exposed`: ripr did not credit the assertion.
- Build a repair route or missing discriminator from the withheld gap.
