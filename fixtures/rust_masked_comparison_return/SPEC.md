# Fixture: rust_masked_comparison_return

Spec: RIPR-SPEC-0158

Owner: analysis-fixtures

Issue: #14709

## Given

A returned comparison changes from `label == "alpha"` to `label == "beta"`.
Both test calls assert the result with the unchanged `on` gate set to false.

## When

The analyzer checks the edit against those two calls.

## Then

The comparison can evaluate differently while the returned Boolean stays false.
The return-value finding reads `propagation_unknown` and names the gate.
