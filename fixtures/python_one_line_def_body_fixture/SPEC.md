# Fixture: python_one_line_def_body_fixture

Spec: RIPR-SPEC-0238
Spec: RIPR-SPEC-0108

## Given

The whole test body sits on its `def` line: `def test_bulk_discount_at_threshold(quantity): assert bulk_discount(quantity) == 0.1`. `quantity` is a conftest fixture that doubles its `params=` rows, so the test calls `bulk_discount(200)` and `bulk_discount(400)`. Both give `0.1` under `>= 100` and under `> 100`.

## When

The boundary changes from `>= 100` to `> 100`.

## Then

One `static_unknown` finding with `unresolved_pytest_fixture`. The body after the signature's colon uses the fixture, so the fixture limit applies.

## Must Not

Skip the fixture use because its line starts with `def`, and report the boundary as `exposed`. The honesty corpus rejects that false promotion independently of the unit tests (#10685).
