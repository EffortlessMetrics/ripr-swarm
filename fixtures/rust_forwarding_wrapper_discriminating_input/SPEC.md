# Fixture: rust_forwarding_wrapper_discriminating_input

Spec: RIPR-SPEC-0197

Owner: analysis-fixtures

Issue: #15034

## Given

The same private helper change and forwarding wrapper as
`rust_forwarding_wrapper_invisible_input`. The integration test asserts
`cash_total(103) == 105` and `cash_total(102) == 100`. Input `103` gives `100`
on the old helper and `105` on the new one.

## When

`cargo xtask fixtures rust_forwarding_wrapper_discriminating_input` runs the
public diff analyzer.

## Then

The return-value finding reads `exposed`. One asserted input separates the
versions, so the forwarding-wrapper invisibility reading does not apply.

## Must Not

- Withhold credit when any asserted wrapper input reaches a difference.
