# Fixture: python_unsettled_indirect_parametrize

Spec: RIPR-SPEC-0238
Spec: RIPR-SPEC-0108

## Given

The test is parametrized with `[100]` and `indirect=IND`, where `IND = True` is a module name rather than a literal. pytest therefore routes 100 through the conftest fixture `quantity`, which doubles it, and the test calls `bulk_discount(200)`. That input gives `0.1` under both `>= 100` and `> 100`.

## When

The boundary changes from `>= 100` to `> 100`.

## Then

One `static_unknown` finding with `unresolved_pytest_fixture`. The argname may reach the test through a fixture, so the literal row 100 is not evidence that the test runs the boundary.

## Must Not

Read 100 as the value the test passes and report the boundary as `exposed`. The honesty corpus rejects that false promotion independently of the unit tests (#10685).
