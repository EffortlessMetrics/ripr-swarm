# Fixture: python_rewrite_shifted_bound

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

`handling_fee` charges 5 below 100. Its one test row, `@pytest.mark.parametrize("amount, fee", [(99, 5)])`, sits on the equality point of the rewritten predicate and asserts the exact fee.

## When

`amount >= 100` is rewritten as `amount > 99`, which is true on the same integers. Run under pytest 9.1.1, the test kills the operator mutant `amount >= 99`, but the mutmut-style shifted bound `amount > 100` survives it, because only an input of 100 tells the two apart.

## Then

One `weakly_exposed` finding that names `amount == 100` as the missing discriminator.

## Must Not

Credit `exposed` from the equality row alone. The honesty corpus rejects that false promotion independently of the unit tests (#10842).
