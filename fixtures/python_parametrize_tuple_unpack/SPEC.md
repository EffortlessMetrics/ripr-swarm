# Fixture: python_parametrize_tuple_unpack

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

A test parametrized by one tuple argname, `@pytest.mark.parametrize("case", [(50, 0.0), (500, 0.1)])`, unpacks it first: `quantity, rate = case`, then asserts `bulk_discount(quantity) == rate`. Neither row sits on the `100` boundary, so the `>= 100` to `> 100` mutant survives both cases.

## When

The boundary changes from `>= 100` to `> 100`.

## Then

One `weakly_exposed` finding. The unpack reads as `parametrize("quantity, rate", ...)`, so the boundary check sees `quantity` values 50 and 500 and reports the missing `quantity == 100` case.

## Must Not

Leave the owner call's `quantity` unbound and keep the oracle's `exposed` verdict through `boundary_activation_unresolved`. The honesty corpus rejects that false promotion independently of the unit tests (#10832).
