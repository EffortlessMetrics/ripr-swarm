# Fixture: rust_forwarding_wrapper_invisible_input

Spec: RIPR-SPEC-0197

Owner: analysis-fixtures

Issue: #15034

## Given

A private helper changes from `cents / 5 * 5` to `(cents + 2) / 5 * 5`. Its
only caller is `pub fn cash_total(cents: u32) -> u32 { round_to_nickel(cents) }`,
which passes its argument straight through. The integration test asserts
`cash_total(105) == 105` and `cash_total(100) == 100`. Both inputs give the
same result on the old and new helper, so the test passes on both.

## When

`cargo xtask fixtures rust_forwarding_wrapper_invisible_input` runs the public
diff analyzer.

## Then

The return-value finding is not `exposed`. Infection reads `weak`: every
asserted call of the forwarding wrapper folds the helper to the same value on
both versions.

## Must Not

- Read a wrapper that changes its argument, a helper with a second caller, or
  a wrapper that something else calls as forwarding. Those keep today's
  reading.
