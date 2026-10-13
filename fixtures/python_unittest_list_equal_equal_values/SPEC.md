# Fixture: python_unittest_list_equal_equal_values

Spec: RIPR-SPEC-0028

## Given

`labels` changes `[1, 1]` to `[1, True]`; a standard unittest assertion expects
literal `[1, True]`. Python list equality considers both values equal. Native
runs register one passing test for each version. This source-derived control
uses the CPython pin in `../python_unittest_list_equal/ORACLE.md`.

## When

`cargo xtask fixtures python_unittest_list_equal_equal_values`

## Then

One `weakly_exposed` finding retains the valid strong exact-value oracle but
withholds discrimination credit for an equality-indistinguishable change.

## Must Not

Report `exposed` or discrimination yes. RIPR-SPEC-0108 independently pins the
classification, oracle, non-promotion and nonempty result.

The existing assertion cannot be prescribed as its own repair. No typed missing
discriminator, repair placement, repair card or strengthen-test next step is
grounded for this equality-indistinguishable edit; preserve the warning and
explicit equality limitation.
